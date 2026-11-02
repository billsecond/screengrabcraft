# The background model

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
