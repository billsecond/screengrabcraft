//! Makes a model file about four times smaller by keeping its weights as 8-bit integers, one
//! scale for each output channel, so it can ship with the program. The result is a plain ONNX
//! model: each weight becomes `Cast` (to float) and `Mul` (by its scales) on two smaller
//! initializers, which tract works out once when it readies the model, so it runs exactly as
//! fast as the original.
//!
//! `cargo run --release -p screengrabcraft-matte --example shrink_model -- IN.onnx OUT.onnx [PICTURE...]`
//!
//! With pictures, it also compares the two models' mattes on them (how far apart, 0 to 255).
//! This is how `assets/models/isnet-general-use-int8.onnx` is made from rembg's
//! `isnet-general-use.onnx` (see `assets/models/README.md`).

use prost::Message;
use screengrabcraft_matte::{Family, Remover};
use screengrabcraft_render::matte::Matting;
use tract_onnx::pb;

/// Weights with fewer values than this stay as they are (biases, norms: small, and touchy).
const SMALLEST: usize = 1024;

fn floats(t: &pb::TensorProto) -> Option<Vec<f32>> {
    if !t.float_data.is_empty() {
        return Some(t.float_data.clone());
    }
    if !t.raw_data.len().is_multiple_of(4) {
        return None;
    }
    Some(t.raw_data.as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b)).collect())
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(input), Some(output)) = (args.first(), args.get(1)) else {
        return Err("usage: shrink_model IN.onnx OUT.onnx [PICTURE...]".to_string());
    };
    let bytes = std::fs::read(input).map_err(|e| format!("{input}: {e}"))?;
    let mut model = pb::ModelProto::decode(bytes.as_slice()).map_err(|e| e.to_string())?;
    let graph = model.graph.as_mut().ok_or("the model has no graph")?;
    let float = pb::tensor_proto::DataType::Float as i32;
    let mut kept = Vec::new();
    let mut nodes = Vec::new();
    let mut shrunk = Vec::new();
    let (mut before, mut after) = (0usize, 0usize);
    for t in std::mem::take(&mut graph.initializer) {
        let values = (t.data_type == float).then(|| floats(&t)).flatten();
        let channels = t.dims.first().copied().unwrap_or(0).max(0) as usize;
        let Some(values) = values.filter(|v| v.len() >= SMALLEST && channels > 0 && v.len() % channels == 0) else {
            kept.push(t);
            continue;
        };
        before += values.len() * 4;
        let per = values.len() / channels;
        let mut q = Vec::with_capacity(values.len());
        let mut scales = Vec::with_capacity(channels);
        for channel in values.chunks_exact(per) {
            let most = channel.iter().fold(0f32, |m, v| m.max(v.abs()));
            let scale = if most > 0.0 { most / 127.0 } else { 1.0 };
            scales.push(scale);
            q.extend(channel.iter().map(|v| (v / scale).round().clamp(-127.0, 127.0) as i8 as u8));
        }
        after += q.len() + scales.len() * 4;
        let name = t.name.clone();
        let mut scale_dims = vec![1i64; t.dims.len()];
        if let Some(d) = scale_dims.first_mut() {
            *d = channels as i64;
        }
        kept.push(pb::TensorProto {
            name: format!("{name}::q"),
            dims: t.dims.clone(),
            data_type: pb::tensor_proto::DataType::Int8 as i32,
            raw_data: q,
            ..Default::default()
        });
        kept.push(pb::TensorProto {
            name: format!("{name}::scale"),
            dims: scale_dims,
            data_type: float,
            raw_data: scales.iter().flat_map(|s| s.to_le_bytes()).collect(),
            ..Default::default()
        });
        nodes.push(pb::NodeProto {
            input: vec![format!("{name}::q")],
            output: vec![format!("{name}::f")],
            name: format!("{name}::cast"),
            op_type: "Cast".to_string(),
            attribute: vec![pb::AttributeProto {
                name: "to".to_string(),
                r#type: pb::attribute_proto::AttributeType::Int as i32,
                i: i64::from(float),
                ..Default::default()
            }],
            ..Default::default()
        });
        nodes.push(pb::NodeProto {
            input: vec![format!("{name}::f"), format!("{name}::scale")],
            output: vec![name.clone()],
            name: format!("{name}::dequantize"),
            op_type: "Mul".to_string(),
            ..Default::default()
        });
        shrunk.push(name);
    }
    graph.initializer = kept;
    // Old files list their weights among the inputs too; those are made by the new nodes now.
    graph.input.retain(|i| !shrunk.contains(&i.name));
    nodes.append(&mut graph.node);
    graph.node = nodes;
    model.producer_name = format!("{} (weights to 8 bits by ScreenGrabCraft)", model.producer_name);
    let out = model.encode_to_vec();
    std::fs::write(output, &out).map_err(|e| format!("{output}: {e}"))?;
    println!(
        "{} weights: {:.1} MB to {:.1} MB; the file {:.1} MB to {:.1} MB",
        shrunk.len(),
        before as f64 / 1e6,
        after as f64 / 1e6,
        bytes.len() as f64 / 1e6,
        out.len() as f64 / 1e6
    );

    let pictures = args.get(2..).unwrap_or_default();
    if pictures.is_empty() {
        return Ok(());
    }
    let family = Family::from_name(input).ok_or("which family is the model?")?;
    let original = Remover::new("original", family, Box::new(move || Ok(bytes.clone())));
    let small = Remover::new("shrunk", family, Box::new(move || Ok(out.clone())));
    println!("| picture | mean difference | largest | pixels more than 32 apart |");
    println!("|---|---|---|---|");
    for p in pictures {
        let pic = screengrabcraft_render::decode(&std::fs::read(p).map_err(|e| format!("{p}: {e}"))?).map_err(|e| e.to_string())?;
        let a = original.matte(&pic, &mut |_| true).map_err(|e| e.to_string())?;
        let b = small.matte(&pic, &mut |_| true).map_err(|e| e.to_string())?;
        let diffs: Vec<u8> = a.alpha().iter().zip(b.alpha()).map(|(x, y)| x.abs_diff(*y)).collect();
        let mean = diffs.iter().map(|d| f64::from(*d)).sum::<f64>() / diffs.len().max(1) as f64;
        let most = diffs.iter().max().copied().unwrap_or(0);
        let far = diffs.iter().filter(|d| **d > 32).count() as f64 / diffs.len().max(1) as f64 * 100.0;
        let name = std::path::Path::new(p).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        println!("| {name} | {mean:.2} | {most} | {far:.3}% |");
    }
    Ok(())
}
