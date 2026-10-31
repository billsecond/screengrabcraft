# ScreenGrabCraft app icons

Two icons, one for each app, both flat pictures of what the apps do: a selection being dragged out
on a colour field.

- **ScreenGrabCraft** (the tray capture service): a teal selection with white corner handles on a
  dark field, and a coral arrow cursor dragging its fourth corner.
- **ScreenGrabCraft Editor** (the annotation editor): the same selection in pale lilac on indigo,
  with a coral pencil in place of the cursor.

## Colours

| Colour | Hex | Used for |
|---|---|---|
| Capture field | `#1d2a38` to `#0d141c` | the capture service's background, top left to bottom right |
| Teal | `#2cd6be` | the capture service's selection |
| Editor field | `#6a52db` to `#2d2170` | the editor's background |
| Lilac | `#e4deff` | the editor's selection |
| Coral | `#ff6a4d` | the cursor and the pencil |
| White | `#ffffff` | the corner handles |

The pencil also has `#f7e7c6` wood, an `#f7f3ff` band and an `#ffb3a6` eraser, and the editor's dark
edge is `#1e1650`.

## Geometry

A rounded square with a corner radius of 22.5% of its side, filled to the edge. The macOS files
pad it onto Apple's 824/1024 icon grid. The sizes up to 32 px draw the same picture with heavier
lines and a bigger cursor or pencil so it still reads.

## Where they come from

The artwork is drawn in Rust, in `xtask/src/art.rs`: every shape is a sum sampled at each point,
and `xtask/src/icon.rs` averages those samples into pixels. Nothing is traced, copied or generated
by an image model, and no other product's imagery is used. It is original work of the
ScreenGrabCraft contributors, under the project's licence (MIT OR Apache-2.0).

`cargo xtask icon` draws both and writes every file here, for each of `screengrabcraft` and
`screengrabcraft-editor`:

- the `hicolor` PNGs for Linux (`io.github.billsecond.screengrabcraft.png` and
  `io.github.billsecond.screengrabcraft.editor.png`, 16 to 512 px);
- `<name>.ico` for Windows (16, 24, 32, 48, 64, 128 and 256 px);
- `<name>.icns` and `<name>-macos-512.png` for macOS;
- `<name>-1024.png`.

## Changing them

Edit the drawings in `xtask/src/art.rs`, then:

```sh
cargo xtask icon
```
