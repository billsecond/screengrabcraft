//! Telling a picture's subject from its background with a salient-object model, for "Remove
//! Background".
//!
//! The models are the openly licensed ones rembg made popular, as ONNX files: IS-Net general use
//! and U²-Net (Apache-2.0), BiRefNet (MIT). They run on tract, the ONNX runtime written in Rust
//! (no onnxruntime, no C++), on every core. The model looks at the whole picture shrunk to its
//! square; its answer is stretched back to the picture's size as a soft matte.
//!
//! Layer L3: no UI and no files. The host reads the model file and hands its bytes over
//! ([`Remover::new`]); the first matte reads it and readies it (seconds), so do that off the
//! window's thread. Whatever a model file holds, a bad one is an error, never a crash: tract is
//! run under `catch_unwind`.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod sample;

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use screengrabcraft_model::Raster;
pub use screengrabcraft_render::matte::{Matte, MatteError, Matting};
use tract_onnx::prelude::*;
use tract_onnx::tract_core::internal::OpState;
use tract_onnx::tract_core::plan::{self, EvalContext, SimpleState};
use tract_onnx::tract_hir::infer::Factoid;

/// The largest model file read (the biggest of the families is about 230 MB).
pub const MAX_MODEL_BYTES: usize = 1024 * 1024 * 1024;

/// How a family of models wants its picture and gives its answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    /// U²-Net and U²-Netp (320 × 320, ImageNet's mean and spread).
    U2Net,
    /// IS-Net (1024 × 1024, centred on grey).
    IsNet,
    /// BiRefNet (1024 × 1024, ImageNet's mean and spread, answers before the sigmoid).
    BiRefNet,
}

impl Family {
    /// The side of the square picture the model looks at.
    pub fn side(self) -> usize {
        match self {
            Family::U2Net => 320,
            Family::IsNet | Family::BiRefNet => 1024,
        }
    }

    fn mean_spread(self) -> ([f32; 3], [f32; 3]) {
        match self {
            Family::U2Net | Family::BiRefNet => ([0.485, 0.456, 0.406], [0.229, 0.224, 0.225]),
            Family::IsNet => ([0.5; 3], [1.0; 3]),
        }
    }

    /// The family a model file's name says it is of (`isnet-general-use.onnx`, `u2netp.onnx`).
    pub fn from_name(name: &str) -> Option<Family> {
        let n = name.to_ascii_lowercase();
        if n.contains("isnet") {
            Some(Family::IsNet)
        } else if n.contains("birefnet") {
            Some(Family::BiRefNet)
        } else if n.contains("u2net") || n.contains("silueta") {
            Some(Family::U2Net)
        } else {
            None
        }
    }
}

type Plan = Arc<TypedSimplePlan>;

/// A model read and readied to run.
pub struct Model {
    family: Family,
    plan: Plan,
}

fn failed(e: impl std::fmt::Display) -> MatteError {
    MatteError::Model(format!("{e:#}"))
}

/// Runs `f`, turning a panic in it (in tract, on a file it didn't expect) into an error.
fn guarded<T>(what: &str, f: impl FnOnce() -> Result<T, MatteError>) -> Result<T, MatteError> {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|_| {
        log::error!("{what} panicked");
        Err(MatteError::Model(format!("{what} failed unexpectedly")))
    })
}

/// The cores tract works on: one pool for every model, made once.
fn executor() -> Option<multithread::Executor> {
    static POOL: OnceLock<Option<multithread::Executor>> = OnceLock::new();
    POOL.get_or_init(|| {
        let n = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 16);
        catch_unwind(|| multithread::Executor::multithread_with_name(n, "matte")).ok()
    })
    .clone()
}

impl Model {
    /// Reads an ONNX model of `family` from its bytes and readies it (seconds for the large
    /// ones).
    pub fn load(family: Family, bytes: &[u8]) -> Result<Model, MatteError> {
        if bytes.len() > MAX_MODEL_BYTES {
            return Err(MatteError::Model("the model file is too large".to_string()));
        }
        guarded("reading the model", || {
            let side = family.side();
            let model = tract_onnx::onnx().model_for_read(&mut std::io::Cursor::new(bytes)).map_err(failed)?;
            // A model that says what size it looks at must look at the family's.
            let wanted = model.input_fact(0).map_err(failed)?.shape.dims().map(|d| d.concretize().and_then(|d| d.to_i64().ok())).collect::<Vec<_>>();
            if wanted.len() != 4 || wanted.iter().skip(2).flatten().any(|d| *d != side as i64) {
                return Err(MatteError::Model(format!("the model doesn't look at {side} × {side} pictures, as a {family:?} model does")));
            }
            let model = model.with_input_fact(0, f32::fact([1, 3, side, side]).into()).map_err(failed)?.into_optimized().map_err(failed)?;
            let options = RunOptions { executor: executor(), ..RunOptions::default() };
            let plan = model.into_runnable_with_options(&options).map_err(failed)?;
            Ok(Model { family, plan })
        })
    }

    pub fn family(&self) -> Family {
        self.family
    }

    /// The picture's matte. `progress` hears how far it has got (0 to 1) and answers whether
    /// to go on.
    pub fn run(&self, picture: &Raster, progress: &mut dyn FnMut(f32) -> bool) -> Result<Matte, MatteError> {
        let side = self.family.side();
        let input = sample::input(picture, side, self.family.mean_spread())?;
        let tensor = Tensor::from_shape(&[1, 3, side, side], &input).map_err(failed)?;
        let mut cancelled = false;
        let answer = guarded("removing the background", || {
            let mut state = SimpleState::new(&self.plan).map_err(failed)?;
            let steps = self.plan.order_without_consts().len().max(1) as f32;
            let mut step = 0.0f32;
            let outputs = state
                .run_plan_with_eval(
                    tvec!(tensor.into_tvalue()),
                    |ctx: &EvalContext, op: Option<&mut (dyn OpState + 'static)>, node: &TypedNode, inputs: TVec<TValue>| {
                        step += 1.0;
                        if !progress((step / steps).min(1.0)) {
                            cancelled = true;
                            return Err(tract_onnx::tract_core::internal::format_err!("cancelled"));
                        }
                        plan::eval(ctx, op, node, inputs)
                    },
                )
                .map_err(failed)?;
            let first = outputs.first().ok_or_else(|| MatteError::Model("the model gave no answer".to_string()))?;
            let values: Vec<f32> = first.to_plain_array_view::<f32>().map_err(failed)?.iter().take(side * side).copied().collect();
            if values.len() < side * side {
                return Err(MatteError::Model("the model's answer is the wrong size".to_string()));
            }
            Ok(values)
        });
        let mut values = match answer {
            Err(_) if cancelled => return Err(MatteError::Cancelled),
            other => other?,
        };
        if self.family == Family::BiRefNet {
            for v in &mut values {
                *v = 1.0 / (1.0 + (-*v).exp());
            }
        }
        let small = sample::to_matte(&values, side)?;
        Ok(small.resized(picture.width(), picture.height())?)
    }
}

/// Where a [`Remover`] gets its model file's bytes.
pub type Source = Box<dyn Fn() -> Result<Vec<u8>, String> + Send + Sync>;

/// A model as a host lends it to the editor: named, read and readied the first time it is
/// used (on whatever thread asks), then kept.
pub struct Remover {
    name: String,
    family: Family,
    source: Source,
    model: Mutex<Option<Arc<Model>>>,
}

impl Remover {
    /// A model of `family` called `name`, its file read by `source` when first needed.
    pub fn new(name: impl Into<String>, family: Family, source: Source) -> Remover {
        Remover { name: name.into(), family, source, model: Mutex::new(None) }
    }

    /// The model, read and readied if it isn't yet.
    pub fn model(&self) -> Result<Arc<Model>, MatteError> {
        let mut slot = self.model.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(m) = slot.as_ref() {
            return Ok(m.clone());
        }
        let bytes = (self.source)().map_err(MatteError::Model)?;
        let started = std::time::Instant::now();
        let m = Arc::new(Model::load(self.family, &bytes)?);
        log::info!("background model {} ready in {:.1} s", self.name, started.elapsed().as_secs_f32());
        *slot = Some(m.clone());
        Ok(m)
    }
}

impl Matting for Remover {
    fn name(&self) -> &str {
        &self.name
    }

    fn matte(&self, picture: &Raster, progress: &mut dyn FnMut(f32) -> bool) -> Result<Matte, MatteError> {
        // Readying the model is the first fifth of the way when it is needed.
        let ready = self.model.lock().unwrap_or_else(PoisonError::into_inner).is_some();
        let start = if ready { 0.0 } else { 0.2 };
        if !progress(0.0) {
            return Err(MatteError::Cancelled);
        }
        let model = self.model()?;
        model.run(picture, &mut |f| progress(start + (1.0 - start) * f))
    }
}

#[cfg(test)]
mod tests;
