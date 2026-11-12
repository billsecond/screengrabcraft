# The models

Two models are installed with ScreenGrabCraft and never downloaded: Remove Background's
(`isnet-general-use-int8.onnx`) and the speech model that makes the captions of recordings
(`whisper-base.en-q8_0.gguf`, [below](#the-speech-model)).

## The background model

`isnet-general-use-int8.onnx` is what **Image › Remove Background** runs: IS-Net, the "general
use" weights, which tell a picture's subject from its background. It is installed with the editor
and the command-line program and never downloaded.

| | |
|---|---|
| Model | IS-Net, from *Highly Accurate Dichotomous Image Segmentation* (Xuebin Qin, Hang Dai, Xiaobin Hu, Deng-Ping Fan, Ling Shao, Luc Van Gool; ECCV 2022) |
| Weights | `isnet-general-use.pth`, released by the authors with https://github.com/xuebinqin/DIS |
| Licence | Apache-2.0 (the DIS repository's `LICENSE.md`; the full text is `LICENSE-APACHE` at the root of this repository) |
| ONNX file it was made from | `isnet-general-use.onnx` from rembg (Daniel Gatis, MIT), https://github.com/danielgatis/rembg/releases/download/v0.0.0/isnet-general-use.onnx, SHA-256 `60920e99c45464f2ba57bee2ad08c919a52bbf852739e96947fbb4358c0d964a` (178.6 MB) |
| This file | 46.7 MB, SHA-256 `547d5a43bec65e1e3eea13d3cb62560f99d1c6ac6afd8f8d28f01dfde2e78351` |

## How it was made

The weights are kept in 8 bits, with one scale for each output channel, so the file is a quarter
of the size. It is still a plain ONNX model: each weight is a `Cast` and a `Mul` of two smaller
tensors, which tract works out once when it reads the model, so it runs as fast as the original.

```sh
cargo run --release -p screengrabcraft-matte --example shrink_model -- \
    isnet-general-use.onnx assets/models/isnet-general-use-int8.onnx [pictures to compare on...]
```

On four photos (CC0 and public domain, from scikit-image's sample data) its mattes are, on
average, less than one step in 255 from the original's; the largest difference was 22 steps, at
the soft edge of a subject.

## Another model

A model file put in the `models` folder in the settings folder (`%APPDATA%\ScreenGrabCraft\models`,
`~/Library/Application Support/ScreenGrabCraft/models`, `~/.config/screengrabcraft/models`) is
used instead, if its name says which family it is: `isnet…` (IS-Net, 1024 × 1024), `u2net…`
(U²-Net and U²-Netp, 320 × 320) or `birefnet…` (BiRefNet, 1024 × 1024). Compare models with
`cargo run --release -p screengrabcraft-matte --example compare`.

## The speech model

`whisper-base.en-q8_0.gguf` is what makes the **captions** of Video Production recordings: what
you said into the microphone ("You") and what the others said through the computer ("Others").
The capture service runs it after a recording, on this computer, on one thread at the lowest
priority, as a program of its own (`screengrabcraft --transcribe <recording>`); nothing is sent
anywhere. It is installed beside the capture service and never downloaded.

| | |
|---|---|
| Model | Whisper base, English only (74 million weights), from *Robust Speech Recognition via Large-Scale Weak Supervision* (Alec Radford, Jong Wook Kim, Tao Xu, Greg Brockman, Christine McLeavey, Ilya Sutskever; OpenAI, 2022) |
| Weights | `openai/whisper-base.en` as OpenAI publishes it on Hugging Face (https://huggingface.co/openai/whisper-base.en): `model.safetensors` SHA-256 `d4dd5542fd6a1d35639e21384238f3bfe6c557c849d392b5905d33ee29e71db5`, `config.json` `160c1df40a60d4ef5a6014d536fd845c33aabfd1f318458ac7f87cfe91ebbe76`, `tokenizer.json` `5eb60cec1e77aeeb6869a2bb5a8e01a84c3fe5d072d75369343021fe6f5310d0` |
| Licence | MIT (https://github.com/openai/whisper; the notice is `LICENSE-WHISPER.txt`, installed beside the model) |
| This file | 83.8 MB, SHA-256 `08c6a80bcb3b3ff8cae0b82413ae14d85cd4761c34093fe881f71b687bb7bd6e` |

### How it was made

Every matrix of weights is kept in 8 bits in blocks of 32 (`Q8_0`, a scale for each block); the
rest (the convolutions, the norms, the positions) stays in 32-bit floats. The vocabulary and the
numbers the decoder needs (its special tokens, the tokens it never writes) are written into the
same file, so it is the only file needed. The same three files make the same model file, byte
for byte:

```sh
cargo run --release -p screengrabcraft-speech --example convert_model -- \
    <folder with model.safetensors, config.json, tokenizer.json> assets/models/whisper-base.en-q8_0.gguf
```

It runs on candle, the tensor library written in Rust (`crates/speech`): the decoder on candle's
8-bit kernels, the encoder widened back to 32 bits as it is read (candle's float kernels are the
quicker for its large matrices).

### How well and how fast

Measured with speech that is nobody's own (made-up sentences read by Windows' text-to-speech
voices), on one thread at the lowest priority, on the computer the pull request was made on
(`cargo run --release -p screengrabcraft-speech --example bench`):

| Recording | Heard in | Processor time | Words wrong |
|---|---|---|---|
| 1 minute, one voice | 5.9 s | 6.0 s | 1.6 % |
| 10 minutes, one voice | 51 s | 51 s | 0.2 % |
| 4.6 minutes, two people, their voices heard again faintly through the speakers | 22 s | 22 s | 0 % for each, no lines heard twice |

Choosing it: tract (which runs Remove Background) ran the same model, as OpenAI's ONNX export,
2.4 times slower (127 s for the ten minutes on one thread, 129 s on two), its decoder five times
slower per word; its 8-bit ONNX files were 24 times slower still.

### Another model

A `whisper….gguf` file put in the `models` folder in the settings folder is used instead (the
first by name). Another of OpenAI's English-only models (`tiny.en`, `small.en`, `medium.en`) can
be made with `convert_model`; the many-language ones aren't heard rightly yet (the decoder
would need to be told the language). None other is installed: each is larger or less right.
