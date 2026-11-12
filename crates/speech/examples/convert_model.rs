//! Makes the speech model file from OpenAI's Whisper weights as Hugging Face publishes them
//! (`model.safetensors`, `config.json`, `tokenizer.json`): the large weights in 8 bits, the
//! vocabulary and what the decoder needs to know beside them, in one GGUF file.
//!
//! ```sh
//! cargo run --release -p screengrabcraft-speech --example convert_model -- \
//!     <folder with model.safetensors, config.json, tokenizer.json> assets/models/whisper-base.en-q8_0.gguf
//! ```
//!
//! The same files make the same model file, byte for byte.

use std::collections::BTreeMap;

use candle_core::Device;
use screengrabcraft_speech::gguf::{self, Value};
use screengrabcraft_speech::model;

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [dir, out] = args.as_slice() else {
        return Err("usage: convert_model <hugging face folder> <out.gguf>".to_string());
    };
    let dir = std::path::Path::new(dir);
    let read_json = |name: &str| -> Result<serde_json::Value, String> {
        let b = std::fs::read(dir.join(name)).map_err(|e| format!("{name}: {e}"))?;
        serde_json::from_slice(&b).map_err(|e| format!("{name}: {e}"))
    };
    let config = read_json("config.json")?;
    let tokenizer = read_json("tokenizer.json")?;
    let num = |key: &str| config.get(key).and_then(serde_json::Value::as_u64).ok_or(format!("config.json has no {key}"));
    let vocab_size = num("vocab_size")? as usize;
    // The vocabulary, in token order: the BPE pieces, then the added (special) tokens.
    let mut tokens = vec![String::new(); vocab_size];
    let mut by_text = BTreeMap::new();
    for (text, id) in tokenizer["model"]["vocab"].as_object().ok_or("tokenizer.json has no vocabulary")? {
        if let Some(slot) = id.as_u64().and_then(|i| tokens.get_mut(i as usize)) {
            *slot = text.clone();
            by_text.insert(text.clone(), id.as_u64().unwrap_or(0) as u32);
        }
    }
    for t in tokenizer["added_tokens"].as_array().ok_or("tokenizer.json has no added tokens")? {
        let (Some(id), Some(text)) = (t["id"].as_u64(), t["content"].as_str()) else { continue };
        if let Some(slot) = tokens.get_mut(id as usize) {
            *slot = text.to_string();
            by_text.insert(text.to_string(), id as u32);
        }
    }
    let special = |text: &str| by_text.get(text).copied().ok_or(format!("the vocabulary has no {text}"));
    let numbers = |key: &str| -> Value {
        Value::Array(
            config
                .get(key)
                .and_then(serde_json::Value::as_array)
                .map_or(Vec::new(), |a| a.iter().filter_map(serde_json::Value::as_u64).map(|v| Value::U32(v as u32)).collect()),
        )
    };
    let name =
        config.get("_name_or_path").and_then(serde_json::Value::as_str).unwrap_or("whisper").rsplit('/').next().unwrap_or("whisper").to_string();
    let u = |v: u64| Value::U32(v as u32);
    let values = vec![
        ("general.architecture", Value::String("whisper".into())),
        ("general.name", Value::String(name.clone())),
        ("general.license", Value::String("MIT".into())),
        ("general.author", Value::String("OpenAI (Whisper); weights in 8 bits by ScreenGrabCraft contributors".into())),
        ("general.source.url", Value::String(format!("https://huggingface.co/openai/{name}"))),
        ("whisper.n_mels", u(num("num_mel_bins")?)),
        ("whisper.d_model", u(num("d_model")?)),
        ("whisper.encoder.heads", u(num("encoder_attention_heads")?)),
        ("whisper.encoder.layers", u(num("encoder_layers")?)),
        ("whisper.decoder.heads", u(num("decoder_attention_heads")?)),
        ("whisper.decoder.layers", u(num("decoder_layers")?)),
        ("whisper.audio_ctx", u(num("max_source_positions")?)),
        ("whisper.text_ctx", u(num("max_target_positions")?)),
        ("whisper.token.eot", Value::U32(special("<|endoftext|>")?)),
        ("whisper.token.sot", Value::U32(special("<|startoftranscript|>")?)),
        ("whisper.token.no_speech", Value::U32(special("<|nocaptions|>").or_else(|_| special("<|nospeech|>"))?)),
        ("whisper.token.no_timestamps", Value::U32(special("<|notimestamps|>")?)),
        ("whisper.token.timestamp_begin", Value::U32(special("<|notimestamps|>")? + 1)),
        ("whisper.suppress_tokens", numbers("suppress_tokens")),
        ("whisper.begin_suppress_tokens", numbers("begin_suppress_tokens")),
        ("tokenizer.ggml.tokens", Value::Array(tokens.into_iter().map(Value::String).collect())),
    ];
    let weights = candle_core::safetensors::load(dir.join("model.safetensors"), &Device::Cpu).map_err(|e| e.to_string())?;
    let mut names: Vec<&String> = weights.keys().collect();
    names.sort();
    let mut tensors = Vec::with_capacity(names.len());
    for n in names {
        let t = weights[n].to_dtype(candle_core::DType::F32).map_err(|e| e.to_string())?;
        // Positions are added to every input: kept exact.
        let q = if n.contains("embed_positions") {
            candle_core::quantized::QTensor::quantize(&t, candle_core::quantized::GgmlDType::F32).map_err(|e| e.to_string())?
        } else {
            model::quantize(&t).map_err(|e| e.to_string())?
        };
        tensors.push((n.clone(), q));
    }
    let refs: Vec<(&str, &candle_core::quantized::QTensor)> = tensors.iter().map(|(n, t)| (n.as_str(), t)).collect();
    let bytes = gguf::write(&values, &refs).map_err(|e| e.to_string())?;
    std::fs::write(out, &bytes).map_err(|e| format!("{out}: {e}"))?;
    // It reads back as a model.
    let m = screengrabcraft_speech::load(&bytes).map_err(|e| e.to_string())?;
    println!("{out}: {} tensors, {:.1} MB, {} ({:?})", refs.len(), bytes.len() as f64 / 1e6, m.name, m.config);
    Ok(())
}
