//! Whisper (Radford et al., OpenAI, 2022) on candle, on the processor: an encoder that hears 30
//! seconds of sound at once, and a decoder that writes what it heard a token at a time.
//!
//! The model's layout follows OpenAI's `whisper/model.py` and candle-transformers' port of it
//! (MIT or Apache-2.0); unlike that port the decoder keeps its own past keys and values, so a
//! token costs the same however many came before it. The encoder's weights are widened to 32-bit
//! floats when read (its matrices are large, where candle's float kernels are the quicker); the
//! decoder's stay in 8-bit blocks (a token at a time, where those kernels are).

use candle_core::quantized::{QMatMul, QTensor};
use candle_core::{DType, Device, Module, Tensor};
use candle_nn::{Conv1d, Conv1dConfig, LayerNorm};

use crate::SpeechError;
use crate::gguf::{Gguf, Value};

type R<T> = Result<T, SpeechError>;

fn c<T>(r: candle_core::Result<T>) -> R<T> {
    r.map_err(|e| SpeechError::Model(e.to_string()))
}

/// The model's sizes, from its file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub n_mels: usize,
    pub d_model: usize,
    pub encoder_heads: usize,
    pub encoder_layers: usize,
    pub decoder_heads: usize,
    pub decoder_layers: usize,
    /// Sound positions the encoder makes of a window (1500).
    pub audio_ctx: usize,
    /// The most tokens the decoder takes (448).
    pub text_ctx: usize,
    pub vocab: usize,
}

/// The tokens with a meaning of their own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Specials {
    pub eot: u32,
    pub sot: u32,
    pub no_speech: u32,
    pub no_timestamps: u32,
    /// `<|0.00|>`; the timestamps follow it, a fiftieth of a second apart.
    pub timestamp_begin: u32,
    /// Never written.
    pub suppress: Vec<u32>,
    /// Never written first.
    pub begin_suppress: Vec<u32>,
}

/// What Whisper decoding needs of a model (a test can stand in for it).
pub trait Model {
    fn config(&self) -> &Config;
    fn specials(&self) -> &Specials;
    /// Hears a window: its spectrogram, `n_mels × 3000`.
    fn encode(&mut self, mel: &[f32]) -> R<()>;
    /// The scores of every token for the next place, after `token` (the first of a window starts
    /// the decoder afresh).
    fn step(&mut self, token: u32, first: bool) -> R<Vec<f32>>;
    /// The text of some tokens (special ones left out).
    fn text(&self, tokens: &[u32]) -> String;
}

/// The GPT-2 byte-level alphabet: the character each byte is written as in the vocabulary.
fn byte_alphabet() -> Vec<(char, u8)> {
    let mut bytes: Vec<u32> = (u32::from(b'!')..=u32::from(b'~')).chain(0xA1..=0xAC).chain(0xAE..=0xFF).collect();
    let mut chars = bytes.clone();
    let mut n = 0;
    for b in 0..256u32 {
        if !bytes.contains(&b) {
            bytes.push(b);
            chars.push(256 + n);
            n += 1;
        }
    }
    bytes.iter().zip(chars).filter_map(|(b, ch)| Some((char::from_u32(ch)?, u8::try_from(*b).ok()?))).collect()
}

/// Token → its bytes.
pub struct Vocab {
    pieces: Vec<Vec<u8>>,
}

impl Vocab {
    pub fn from_tokens(tokens: &[String]) -> Vocab {
        let alphabet: std::collections::HashMap<char, u8> = byte_alphabet().into_iter().collect();
        Vocab { pieces: tokens.iter().map(|t| t.chars().filter_map(|ch| alphabet.get(&ch).copied()).collect()).collect() }
    }

    pub fn len(&self) -> usize {
        self.pieces.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pieces.is_empty()
    }

    /// The text of the tokens below `upto` (the special ones and the timestamps are above).
    pub fn text(&self, tokens: &[u32], upto: u32) -> String {
        let bytes: Vec<u8> =
            tokens.iter().filter(|t| **t < upto).filter_map(|t| self.pieces.get(*t as usize)).flat_map(|p| p.iter().copied()).collect();
        String::from_utf8_lossy(&bytes).trim().to_string()
    }
}

struct Linear {
    w: QMatMul,
    b: Option<Tensor>,
}

impl Linear {
    fn load(g: &Gguf, name: &str, i: usize, o: usize, bias: bool, widen: bool) -> R<Linear> {
        let q = g.tensor(&format!("{name}.weight"), &[o, i])?;
        let w = if widen { QMatMul::Tensor(c(q.dequantize(&Device::Cpu))?) } else { c(QMatMul::from_qtensor(q))? };
        let b = if bias { Some(vector(g, &format!("{name}.bias"), o)?) } else { None };
        Ok(Linear { w, b })
    }

    fn forward(&self, x: &Tensor) -> R<Tensor> {
        let y = c(self.w.forward(x))?;
        match &self.b {
            Some(b) => c(y.broadcast_add(b)),
            None => Ok(y),
        }
    }
}

fn dense(g: &Gguf, name: &str, dims: &[usize]) -> R<Tensor> {
    c(g.tensor(name, dims)?.dequantize(&Device::Cpu))
}

fn vector(g: &Gguf, name: &str, n: usize) -> R<Tensor> {
    dense(g, name, &[n])
}

fn layer_norm(g: &Gguf, name: &str, n: usize) -> R<LayerNorm> {
    Ok(LayerNorm::new(vector(g, &format!("{name}.weight"), n)?, vector(g, &format!("{name}.bias"), n)?, 1e-5))
}

struct Attention {
    q: Linear,
    k: Linear,
    v: Linear,
    out: Linear,
    heads: usize,
    /// Self-attention: the keys and values so far (heads, scaled). Cross-attention: the sound's
    /// keys (transposed) and values, made once a window.
    cache: Option<(Tensor, Tensor)>,
}

impl Attention {
    fn load(g: &Gguf, name: &str, n: usize, heads: usize, widen: bool) -> R<Attention> {
        Ok(Attention {
            q: Linear::load(g, &format!("{name}.q_proj"), n, n, true, widen)?,
            k: Linear::load(g, &format!("{name}.k_proj"), n, n, false, widen)?,
            v: Linear::load(g, &format!("{name}.v_proj"), n, n, true, widen)?,
            out: Linear::load(g, &format!("{name}.out_proj"), n, n, true, widen)?,
            heads,
            cache: None,
        })
    }

    /// (batch, time, channels) → (batch, heads, time, channels per head).
    fn split(&self, x: &Tensor) -> R<Tensor> {
        let (b, t, ch) = c(x.dims3())?;
        c(c(c(x.reshape((b, t, self.heads, ch / self.heads.max(1))))?.transpose(1, 2))?.contiguous())
    }

    fn scale(&self, x: &Tensor) -> f64 {
        let ch = x.dims().last().copied().unwrap_or(1);
        ((ch / self.heads.max(1)).max(1) as f64).powf(-0.25)
    }

    fn attend(q: &Tensor, kt: &Tensor, v: &Tensor) -> R<Tensor> {
        let w = c(candle_nn::ops::softmax_last_dim(&c(q.matmul(kt))?))?;
        c(c(c(w.matmul(v))?.transpose(1, 2))?.flatten_from(2))
    }

    /// Self-attention; `remember` keeps x's keys and values for the next tokens (the decoder).
    fn itself(&mut self, x: &Tensor, remember: bool) -> R<Tensor> {
        let s = self.scale(x);
        let q = c(self.split(&self.q.forward(x)?)? * s)?;
        let mut k = c(self.split(&self.k.forward(x)?)? * s)?;
        let mut v = self.split(&self.v.forward(x)?)?;
        if remember {
            if let Some((pk, pv)) = &self.cache {
                k = c(Tensor::cat(&[pk, &k], 2))?;
                v = c(Tensor::cat(&[pv, &v], 2))?;
            }
            self.cache = Some((k.clone(), v.clone()));
        }
        self.out.forward(&Attention::attend(&q, &c(k.t())?, &v)?)
    }

    /// Attention to the sound the encoder made of the window.
    fn hear_sound(&mut self, x: &Tensor, sound: &Tensor) -> R<Tensor> {
        let s = self.scale(x);
        let q = c(self.split(&self.q.forward(x)?)? * s)?;
        if self.cache.is_none() {
            let kt = c(c(c(self.split(&self.k.forward(sound)?)? * s)?.t())?.contiguous())?;
            let v = self.split(&self.v.forward(sound)?)?;
            self.cache = Some((kt, v));
        }
        let (kt, v) = self.cache.as_ref().ok_or_else(|| SpeechError::Model("the sound was not heard".to_string()))?;
        self.out.forward(&Attention::attend(&q, kt, v)?)
    }
}

struct Block {
    attn: Attention,
    attn_ln: LayerNorm,
    cross: Option<(Attention, LayerNorm)>,
    fc1: Linear,
    fc2: Linear,
    mlp_ln: LayerNorm,
}

impl Block {
    fn load(g: &Gguf, name: &str, n: usize, heads: usize, decoder: bool) -> R<Block> {
        let widen = !decoder;
        let cross = if decoder {
            Some((
                Attention::load(g, &format!("{name}.encoder_attn"), n, heads, widen)?,
                layer_norm(g, &format!("{name}.encoder_attn_layer_norm"), n)?,
            ))
        } else {
            None
        };
        Ok(Block {
            attn: Attention::load(g, &format!("{name}.self_attn"), n, heads, widen)?,
            attn_ln: layer_norm(g, &format!("{name}.self_attn_layer_norm"), n)?,
            cross,
            fc1: Linear::load(g, &format!("{name}.fc1"), n, 4 * n, true, widen)?,
            fc2: Linear::load(g, &format!("{name}.fc2"), 4 * n, n, true, widen)?,
            mlp_ln: layer_norm(g, &format!("{name}.final_layer_norm"), n)?,
        })
    }

    fn forward(&mut self, x: &Tensor, sound: Option<&Tensor>) -> R<Tensor> {
        let a = self.attn.itself(&c(self.attn_ln.forward(x))?, sound.is_some())?;
        let mut x = c(x + a)?;
        if let (Some((attn, ln)), Some(sound)) = (&mut self.cross, sound) {
            let a = attn.hear_sound(&c(ln.forward(&x))?, sound)?;
            x = c(&x + a)?;
        }
        let h = c(self.fc1.forward(&c(self.mlp_ln.forward(&x))?)?.gelu_erf())?;
        c(x + self.fc2.forward(&h)?)
    }

    fn forget(&mut self) {
        self.attn.cache = None;
        if let Some((a, _)) = &mut self.cross {
            a.cache = None;
        }
    }
}

/// Whisper, read from its file.
pub struct Whisper {
    pub config: Config,
    pub specials: Specials,
    /// What the file calls it (`whisper-base.en`).
    pub name: String,
    vocab: Vocab,
    conv1: Conv1d,
    conv2: Conv1d,
    encoder_pos: Tensor,
    encoder: Vec<Block>,
    encoder_ln: LayerNorm,
    tokens: Tensor,
    logits: QMatMul,
    decoder_pos: Tensor,
    decoder: Vec<Block>,
    decoder_ln: LayerNorm,
    sound: Option<Tensor>,
    position: usize,
}

fn numbers(g: &Gguf, key: &str) -> Vec<u32> {
    match g.value(key) {
        Some(Value::Array(v)) => v.iter().filter_map(Value::as_u64).filter_map(|n| u32::try_from(n).ok()).take(4096).collect(),
        _ => Vec::new(),
    }
}

impl Whisper {
    /// Reads the model from its file's bytes and checks every size before it's used.
    pub fn load(bytes: &[u8]) -> R<Whisper> {
        let g = Gguf::read(bytes)?;
        if g.value("general.architecture").and_then(Value::as_str) != Some("whisper") {
            return Err(SpeechError::Model("this isn't a Whisper model".to_string()));
        }
        let tokens: Vec<String> = match g.value("tokenizer.ggml.tokens") {
            Some(Value::Array(v)) => v.iter().map(|t| t.as_str().unwrap_or_default().to_string()).collect(),
            _ => return Err(SpeechError::Model("the model file has no vocabulary".to_string())),
        };
        let config = Config {
            n_mels: g.number("whisper.n_mels")?,
            d_model: g.number("whisper.d_model")?,
            encoder_heads: g.number("whisper.encoder.heads")?,
            encoder_layers: g.number("whisper.encoder.layers")?,
            decoder_heads: g.number("whisper.decoder.heads")?,
            decoder_layers: g.number("whisper.decoder.layers")?,
            audio_ctx: g.number("whisper.audio_ctx")?,
            text_ctx: g.number("whisper.text_ctx")?,
            vocab: tokens.len(),
        };
        let token = |key: &str| -> R<u32> {
            g.number(&format!("whisper.token.{key}"))
                .ok()
                .and_then(|n| u32::try_from(n).ok())
                .filter(|t| (*t as usize) < config.vocab)
                .ok_or_else(|| SpeechError::Model(format!("the model file's {key} token is missing or outside its vocabulary")))
        };
        let specials = Specials {
            eot: token("eot")?,
            sot: token("sot")?,
            no_speech: token("no_speech")?,
            no_timestamps: token("no_timestamps")?,
            timestamp_begin: token("timestamp_begin")?,
            suppress: numbers(&g, "whisper.suppress_tokens"),
            begin_suppress: numbers(&g, "whisper.begin_suppress_tokens"),
        };
        check(&config, &specials)?;
        let (n, mels) = (config.d_model, config.n_mels);
        let conv = |name: &str, inputs: usize, stride: usize| -> R<Conv1d> {
            let w = dense(&g, &format!("model.encoder.{name}.weight"), &[n, inputs, 3])?;
            let b = vector(&g, &format!("model.encoder.{name}.bias"), n)?;
            Ok(Conv1d::new(w, Some(b), Conv1dConfig { padding: 1, stride, dilation: 1, groups: 1, cudnn_fwd_algo: None }))
        };
        let token_table = g.tensor("model.decoder.embed_tokens.weight", &[config.vocab, n])?;
        let tokens_f32 = c(token_table.dequantize(&Device::Cpu))?;
        let encoder = (0..config.encoder_layers)
            .map(|i| Block::load(&g, &format!("model.encoder.layers.{i}"), n, config.encoder_heads, false))
            .collect::<R<Vec<_>>>()?;
        let decoder = (0..config.decoder_layers)
            .map(|i| Block::load(&g, &format!("model.decoder.layers.{i}"), n, config.decoder_heads, true))
            .collect::<R<Vec<_>>>()?;
        Ok(Whisper {
            name: g.value("general.name").and_then(Value::as_str).unwrap_or("whisper").chars().take(80).collect(),
            vocab: Vocab::from_tokens(&tokens),
            conv1: conv("conv1", mels, 1)?,
            conv2: conv("conv2", n, 2)?,
            encoder_pos: dense(&g, "model.encoder.embed_positions.weight", &[config.audio_ctx, n])?,
            encoder,
            encoder_ln: layer_norm(&g, "model.encoder.layer_norm", n)?,
            tokens: tokens_f32,
            logits: c(QMatMul::from_qtensor(token_table))?,
            decoder_pos: dense(&g, "model.decoder.embed_positions.weight", &[config.text_ctx, n])?,
            decoder,
            decoder_ln: layer_norm(&g, "model.decoder.layer_norm", n)?,
            sound: None,
            position: 0,
            config,
            specials,
        })
    }
}

/// Sizes a Whisper model can have, and specials inside its vocabulary.
fn check(c: &Config, s: &Specials) -> R<()> {
    let bad = |what: &str| Err(SpeechError::Model(format!("the model's {what} is not a Whisper model's")));
    if c.n_mels != 80 && c.n_mels != 128 {
        return bad("number of mel bands");
    }
    if !(16..=2048).contains(&c.d_model) {
        return bad("width");
    }
    for heads in [c.encoder_heads, c.decoder_heads] {
        if heads == 0 || heads > 64 || !c.d_model.is_multiple_of(heads) {
            return bad("number of heads");
        }
    }
    if !(1..=64).contains(&c.encoder_layers) || !(1..=64).contains(&c.decoder_layers) {
        return bad("number of layers");
    }
    if c.audio_ctx != crate::mel::FRAMES / 2 {
        return bad("sound context");
    }
    if !(8..=448).contains(&c.text_ctx) {
        return bad("text context");
    }
    // A timestamp for every fiftieth of a second of a window.
    if (s.timestamp_begin as usize).saturating_add(1501) > c.vocab || c.vocab > 70_000 {
        return bad("vocabulary");
    }
    Ok(())
}

impl Model for Whisper {
    fn config(&self) -> &Config {
        &self.config
    }

    fn specials(&self) -> &Specials {
        &self.specials
    }

    fn encode(&mut self, mel: &[f32]) -> R<()> {
        let frames = crate::mel::FRAMES;
        if mel.len() != self.config.n_mels * frames {
            return Err(SpeechError::Model("a spectrogram of the wrong size".to_string()));
        }
        let x = c(Tensor::from_slice(mel, (1, self.config.n_mels, frames), &Device::Cpu))?;
        let x = c(c(self.conv1.forward(&x))?.gelu_erf())?;
        let x = c(c(c(self.conv2.forward(&x))?.gelu_erf())?.transpose(1, 2))?;
        let mut x = c(x.broadcast_add(&self.encoder_pos))?;
        for b in &mut self.encoder {
            x = b.forward(&x, None)?;
        }
        self.sound = Some(c(self.encoder_ln.forward(&x))?);
        Ok(())
    }

    fn step(&mut self, token: u32, first: bool) -> R<Vec<f32>> {
        if first {
            self.position = 0;
            for b in &mut self.decoder {
                b.forget();
            }
        }
        if self.position >= self.config.text_ctx || token as usize >= self.config.vocab {
            return Err(SpeechError::Model("past the decoder's context".to_string()));
        }
        let sound = self.sound.clone().ok_or_else(|| SpeechError::Model("nothing was heard yet".to_string()))?;
        let ids = c(Tensor::new(&[token], &Device::Cpu))?;
        let x = c(c(self.tokens.index_select(&ids, 0))?.unsqueeze(0))?;
        let mut x = c(x.broadcast_add(&c(self.decoder_pos.narrow(0, self.position, 1))?))?;
        for b in &mut self.decoder {
            x = b.forward(&x, Some(&sound))?;
        }
        self.position += 1;
        let x = c(self.decoder_ln.forward(&x))?;
        let logits = c(c(c(self.logits.forward(&x))?.flatten_all())?.to_dtype(DType::F32))?;
        c(logits.to_vec1())
    }

    fn text(&self, tokens: &[u32]) -> String {
        self.vocab.text(tokens, self.specials.eot)
    }
}

/// The 8-bit form of a weight (what the model file keeps the large ones in).
pub fn quantize(t: &Tensor) -> R<QTensor> {
    let dims = t.dims();
    let wide = dims.len() == 2 && dims.last().is_some_and(|d| d.is_multiple_of(32));
    c(QTensor::quantize(t, if wide { candle_core::quantized::GgmlDType::Q8_0 } else { candle_core::quantized::GgmlDType::F32 }))
}
