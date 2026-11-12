//! The model file: GGUF (version 3), read and written here, every size checked before anything
//! is made from it.
//!
//! ```text
//! file   := "GGUF" version:u32 tensors:u64 values:u64 value* tensor_info* padding data
//! value  := key:string type:u32 payload
//! info   := name:string dims:u32 dim:u64* (innermost first) dtype:u32 offset:u64
//! string := len:u64 bytes
//! ```
//!
//! Only the kinds of weights the model is made of are taken: 32- and 16-bit floats and 8-bit
//! blocks (`Q8_0`). Each tensor's bytes are copied into memory of the alignment candle needs.

use std::collections::BTreeMap;

use candle_core::quantized::{GgmlDType, QTensor, ggml_file};

use crate::SpeechError;

const MAGIC: &[u8; 4] = b"GGUF";
const ALIGN: usize = 32;
const MAX_TENSORS: u64 = 4096;
const MAX_VALUES: u64 = 4096;
const MAX_DIMS: u32 = 4;
const MAX_DIM: u64 = 1 << 24;
/// The most numbers in one tensor.
const MAX_ELEMENTS: u64 = 1 << 28;
/// The longest key or name, and the longest string value.
const MAX_NAME: u64 = 512;
const MAX_STRING: u64 = 64 * 1024;
const MAX_ARRAY: u64 = 1 << 20;

fn bad(what: impl Into<String>) -> SpeechError {
    SpeechError::Model(what.into())
}

/// A value in the file's metadata.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    U32(u32),
    I32(i32),
    U64(u64),
    F32(f32),
    Bool(bool),
    String(String),
    /// Smaller numbers, kept as 64-bit.
    Int(i64),
    Array(Vec<Value>),
}

impl Value {
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Value::U32(v) => Some(u64::from(*v)),
            Value::U64(v) => Some(*v),
            Value::I32(v) => u64::try_from(*v).ok(),
            Value::Int(v) => u64::try_from(*v).ok(),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    fn kind(&self) -> u32 {
        match self {
            Value::Int(_) => 11,
            Value::U32(_) => 4,
            Value::I32(_) => 5,
            Value::F32(_) => 6,
            Value::Bool(_) => 7,
            Value::String(_) => 8,
            Value::Array(_) => 9,
            Value::U64(_) => 10,
        }
    }
}

/// Where a tensor is in the file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TensorInfo {
    /// Outermost first, as candle has them.
    pub dims: Vec<usize>,
    pub dtype: GgmlDType,
    /// In the file.
    start: usize,
    len: usize,
}

/// A model file, read and checked.
pub struct Gguf<'a> {
    bytes: &'a [u8],
    pub values: BTreeMap<String, Value>,
    pub tensors: BTreeMap<String, TensorInfo>,
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], SpeechError> {
        let end = self.at.checked_add(n).filter(|e| *e <= self.b.len()).ok_or_else(|| bad("the model file is cut short"))?;
        let s = self.b.get(self.at..end).ok_or_else(|| bad("the model file is cut short"))?;
        self.at = end;
        Ok(s)
    }
    fn array<const N: usize>(&mut self) -> Result<[u8; N], SpeechError> {
        self.take(N)?.try_into().map_err(|_| bad("the model file is cut short"))
    }
    fn u32(&mut self) -> Result<u32, SpeechError> {
        Ok(u32::from_le_bytes(self.array()?))
    }
    fn u64(&mut self) -> Result<u64, SpeechError> {
        Ok(u64::from_le_bytes(self.array()?))
    }
    fn string(&mut self, max: u64) -> Result<String, SpeechError> {
        let n = self.u64()?;
        if n > max {
            return Err(bad("the model file has a string too long"));
        }
        let s = self.take(usize::try_from(n).map_err(|_| bad("the model file has a string too long"))?)?;
        Ok(String::from_utf8_lossy(s).into_owned())
    }
    fn value(&mut self, kind: u32, depth: u32) -> Result<Value, SpeechError> {
        Ok(match kind {
            0 => Value::Int(i64::from(self.array::<1>()?[0])),
            1 => Value::Int(i64::from(i8::from_le_bytes(self.array()?))),
            2 => Value::Int(i64::from(u16::from_le_bytes(self.array()?))),
            3 => Value::Int(i64::from(i16::from_le_bytes(self.array()?))),
            4 => Value::U32(self.u32()?),
            5 => Value::I32(i32::from_le_bytes(self.array()?)),
            6 => Value::F32(f32::from_le_bytes(self.array()?)),
            7 => Value::Bool(self.array::<1>()?[0] != 0),
            8 => Value::String(self.string(MAX_STRING)?),
            9 => {
                if depth > 0 {
                    return Err(bad("the model file has arrays in arrays"));
                }
                let item = self.u32()?;
                let n = self.u64()?;
                // Every item takes a byte at least: a count past what's left is a lie.
                if n > MAX_ARRAY || n > (self.b.len() - self.at) as u64 {
                    return Err(bad("the model file has an array too long"));
                }
                let mut v = Vec::with_capacity(n as usize);
                for _ in 0..n {
                    v.push(self.value(item, depth + 1)?);
                }
                Value::Array(v)
            }
            10 => Value::U64(self.u64()?),
            11 => Value::Int(i64::from_le_bytes(self.array()?)),
            12 => Value::F32(f64::from_le_bytes(self.array()?) as f32),
            _ => return Err(bad("the model file has a value of an unknown kind")),
        })
    }
}

fn dtype_of(code: u32) -> Option<GgmlDType> {
    match code {
        0 => Some(GgmlDType::F32),
        1 => Some(GgmlDType::F16),
        8 => Some(GgmlDType::Q8_0),
        _ => None,
    }
}

fn code_of(dtype: GgmlDType) -> Option<u32> {
    match dtype {
        GgmlDType::F32 => Some(0),
        GgmlDType::F16 => Some(1),
        GgmlDType::Q8_0 => Some(8),
        _ => None,
    }
}

/// Bytes of a tensor of `elements` numbers of `dtype`, if they come in whole blocks.
fn byte_len(dtype: GgmlDType, elements: u64) -> Option<usize> {
    let block = dtype.block_size() as u64;
    if block == 0 || !elements.is_multiple_of(block) {
        return None;
    }
    usize::try_from((elements / block).checked_mul(dtype.type_size() as u64)?).ok()
}

impl<'a> Gguf<'a> {
    /// Reads a model file's layout; nothing past its header is touched until a tensor is asked for.
    pub fn read(bytes: &'a [u8]) -> Result<Gguf<'a>, SpeechError> {
        let mut r = Reader { b: bytes, at: 0 };
        if r.take(4)? != MAGIC {
            return Err(bad("this is not a GGUF model file"));
        }
        let version = r.u32()?;
        if !(2..=3).contains(&version) {
            return Err(bad(format!("GGUF version {version} is not read here")));
        }
        let (n_tensors, n_values) = (r.u64()?, r.u64()?);
        if n_tensors > MAX_TENSORS || n_values > MAX_VALUES {
            return Err(bad("the model file says it holds too much"));
        }
        let mut values = BTreeMap::new();
        for _ in 0..n_values {
            let key = r.string(MAX_NAME)?;
            let kind = r.u32()?;
            values.insert(key, r.value(kind, 0)?);
        }
        let mut infos = Vec::with_capacity(n_tensors as usize);
        for _ in 0..n_tensors {
            let name = r.string(MAX_NAME)?;
            let n_dims = r.u32()?;
            if n_dims == 0 || n_dims > MAX_DIMS {
                return Err(bad(format!("the tensor {name} has {n_dims} dimensions")));
            }
            let mut dims = Vec::with_capacity(n_dims as usize);
            let mut elements = 1u64;
            for _ in 0..n_dims {
                let d = r.u64()?;
                if d == 0 || d > MAX_DIM {
                    return Err(bad(format!("the tensor {name} has a dimension of {d}")));
                }
                elements = elements.saturating_mul(d);
                dims.push(d as usize);
            }
            if elements > MAX_ELEMENTS {
                return Err(bad(format!("the tensor {name} is too large")));
            }
            dims.reverse();
            let code = r.u32()?;
            let dtype = dtype_of(code).ok_or_else(|| bad(format!("the tensor {name} is of a kind not read here ({code})")))?;
            let len = byte_len(dtype, elements).ok_or_else(|| bad(format!("the tensor {name} doesn't fill whole blocks")))?;
            let offset = r.u64()?;
            infos.push((name, dims, dtype, offset, len));
        }
        let align = values.get("general.alignment").and_then(Value::as_u64).unwrap_or(ALIGN as u64);
        if !(1..=4096).contains(&align) {
            return Err(bad("the model file's alignment is impossible"));
        }
        let data = r.at.div_ceil(align as usize) * align as usize;
        let mut tensors = BTreeMap::new();
        for (name, dims, dtype, offset, len) in infos {
            let start =
                usize::try_from(offset).ok().and_then(|o| o.checked_add(data)).ok_or_else(|| bad(format!("the tensor {name} is past the end")))?;
            if start.checked_add(len).is_none_or(|end| end > bytes.len()) {
                return Err(bad(format!("the tensor {name} is past the end of the file")));
            }
            if tensors.insert(name.clone(), TensorInfo { dims, dtype, start, len }).is_some() {
                return Err(bad(format!("the tensor {name} is in the file twice")));
            }
        }
        Ok(Gguf { bytes, values, tensors })
    }

    pub fn value(&self, key: &str) -> Option<&Value> {
        self.values.get(key)
    }

    /// A number the model needs, from its metadata.
    pub fn number(&self, key: &str) -> Result<usize, SpeechError> {
        self.value(key)
            .and_then(Value::as_u64)
            .and_then(|v| usize::try_from(v).ok())
            .ok_or_else(|| bad(format!("the model file doesn't say its {key}")))
    }

    /// A tensor, which must be `dims` (outermost first).
    pub fn tensor(&self, name: &str, dims: &[usize]) -> Result<QTensor, SpeechError> {
        let info = self.tensors.get(name).ok_or_else(|| bad(format!("the model file has no {name}")))?;
        if info.dims != dims {
            return Err(bad(format!("{name} is {:?}, not {dims:?}", info.dims)));
        }
        let src = self.bytes.get(info.start..info.start + info.len).ok_or_else(|| bad(format!("{name} is past the end of the file")))?;
        // candle reads the blocks in place: they need the alignment of their numbers.
        let mut aligned = vec![0u64; info.len.div_ceil(8)];
        let dst: &mut [u8] = bytemuck::cast_slice_mut(&mut aligned);
        dst.get_mut(..info.len).ok_or_else(|| bad("out of memory"))?.copy_from_slice(src);
        let raw: &[u8] = bytemuck::cast_slice(&aligned);
        let raw = raw.get(..info.len).ok_or_else(|| bad("out of memory"))?;
        ggml_file::qtensor_from_ggml(info.dtype, raw, info.dims.clone(), &candle_core::Device::Cpu).map_err(|e| bad(format!("{name}: {e}")))
    }
}

fn put_string(out: &mut Vec<u8>, s: &str) {
    out.extend((s.len() as u64).to_le_bytes());
    out.extend(s.as_bytes());
}

fn put_value(out: &mut Vec<u8>, v: &Value) {
    match v {
        Value::Int(x) => out.extend(x.to_le_bytes()),
        Value::U32(x) => out.extend(x.to_le_bytes()),
        Value::I32(x) => out.extend(x.to_le_bytes()),
        Value::F32(x) => out.extend(x.to_le_bytes()),
        Value::Bool(x) => out.push(u8::from(*x)),
        Value::String(s) => put_string(out, s),
        Value::U64(x) => out.extend(x.to_le_bytes()),
        Value::Array(items) => {
            out.extend(items.first().map_or(4, Value::kind).to_le_bytes());
            out.extend((items.len() as u64).to_le_bytes());
            for i in items {
                put_value(out, i);
            }
        }
    }
}

/// A model file of these values and tensors (each tensor's dims outermost first).
pub fn write(values: &[(&str, Value)], tensors: &[(&str, &QTensor)]) -> Result<Vec<u8>, SpeechError> {
    let mut out = Vec::new();
    out.extend(MAGIC);
    out.extend(3u32.to_le_bytes());
    out.extend((tensors.len() as u64).to_le_bytes());
    out.extend((values.len() as u64).to_le_bytes());
    for (k, v) in values {
        put_string(&mut out, k);
        out.extend(v.kind().to_le_bytes());
        put_value(&mut out, v);
    }
    let mut data = Vec::new();
    for (name, t) in tensors {
        put_string(&mut out, name);
        let dims = t.shape().dims();
        out.extend((dims.len() as u32).to_le_bytes());
        for d in dims.iter().rev() {
            out.extend((*d as u64).to_le_bytes());
        }
        let code = code_of(t.dtype()).ok_or_else(|| bad(format!("{name} is of a kind not written here")))?;
        out.extend(code.to_le_bytes());
        out.extend((data.len() as u64).to_le_bytes());
        data.extend_from_slice(&t.data().map_err(|e| bad(e.to_string()))?);
        data.resize(data.len().div_ceil(ALIGN) * ALIGN, 0);
    }
    out.resize(out.len().div_ceil(ALIGN) * ALIGN, 0);
    out.extend(data);
    Ok(out)
}
