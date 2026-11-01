# Architecture

> **Last updated:** 2026-10-14 · **Change:** first version

ScreenGrabCraft is a Cargo workspace of six library crates in layers and four programs. A crate
may depend only on crates in lower layers (`cargo xtask layers` checks it), and nothing below L6
knows about a user interface.

```
            apps: screengrabcraft (service)   screengrabcraft-editor   screengrabcraft-cli   screengrabcraft-web
                          │                           │                        │                   │
L6   desktop ─────────────┤                           ├── ui ──────────────────┼───────────────────┤
     (settings, log,      │                           │   (editor, overlay)    │                   │
      instance, service)  │                           │                        │                   │
L4                        │                     engine (Session, undo, command registry)           │
L3   capture (monitors, windows, pixels: xcap · x11rb · portal)                                    │
L2                              render (tiny-skia drawing, text, effects, codecs, PDF, projects, stitching, GIF)
L1                              model (document, shapes, styles, geometry, settings)
```

| Crate | Builds for the browser | Job |
|---|---|---|
| `model` | yes | `Document` (a `Raster` plus `Object`s in drawing order), `Shape` (15 kinds), `Style`, hit testing and handles, `Settings`/`EditorSettings`, hotkeys, file naming |
| `render` | yes | `Renderer` (the document drawn, the picture's pixmap cached), blur/pixelate/spotlight/magnify, text from the bundled fonts as paths, stamps, whole-picture effects, PNG/JPEG/WebP/BMP/GIF/PDF, `.sgcraft` projects, the scrolling `Stitcher`, the `GifWriter` |
| `capture` | no | `freeze()` (every monitor's picture, the windows, the pointer), `grab_monitor`, `grab_on_monitor`, `grabber` (one area over and over, for recordings), `grab_area`, permission |
| `engine` | yes | `Session` (document, selection, undo/redo, interactive edits), `COMMANDS` |
| `ui` | yes | `Editor` (menus, tool bar, canvas, properties, recent strip, dialogs), `Overlay` (the capture overlay), `Services` (what the host provides) |
| `desktop` | no | paths, settings files, logging, one instance and hand-over, clipboard, the sign-in service and supervisor, launching the other program, the tray icon's pixels, the Windows graphics backend |

## The document

A capture opened in the editor is a `Document`: the picture as it was captured (never changed by
annotating) and the annotations on it, bottom first. Annotations are vector objects in picture
pixels; `render::draw` draws them in order, so the effect kinds (blur, pixelate, spotlight,
magnify) work on whatever was drawn before them. Whole-picture changes (crop, resize, turn, flip,
canvas size, border, drop shadow) change the picture and move the annotations to match; colour
effects change only the picture. Flattening draws the annotations into the picture for good.

A project file (`.sgcraft`) is JSON: the picture and placed pictures as base64 PNG, the
annotations as the model serialises them. Everything in one is checked as it is read.

## Commands and undo

Every editing action is a `CommandSpec` (id, label, group, shortcut, parameter doc, enabled, run).
`Session::execute` keeps a snapshot first and puts it back if the command fails or panics, and the
snapshot becomes the undo step if the document changed. A drag on the canvas is bracketed by
`Session::begin` and `commit`, so it is one undo step however many frames it took. Pictures are
shared between snapshots (`Arc`), so undo costs little memory unless the picture itself changed.

## The capture service

One process (`apps/screengrabcraft`) with eframe's event loop on the main thread, which the tray
(Windows, macOS) and the global shortcuts (Windows, macOS) need. Its root window is a 1×1
see-through, click-through window shown only while something is on screen, because eframe opens
new windows only from a shown one (while hidden only `App::logic` runs) and paints a hidden window
at most 10 times a second on Windows. What the owner sees are immediate viewports: the overlay on
every monitor, the settings window, the countdown, the recording control and the notices.

A capture: a shortcut, a tray click or a hand-over (`--capture region` from another start) →
`freeze()` on a worker thread (every monitor's picture, the windows front to back, the pointer)
→ the overlay, one borderless always-on-top window per monitor (borderless full screen on the
right monitor everywhere but macOS, where full screen would slide to a Space) showing the frozen
picture → the owner's choice → the picture is cut from that monitor's picture in its own pixels
(nothing is lost to scaling on a Retina or 150% display) → saved, copied and/or opened in the
editor, the file work on a worker thread. GIF and scrolling captures grab the chosen area of one
monitor repeatedly on a thread of their own.

The service is kept running by the system's own means where there are any (launchd's `KeepAlive`,
systemd's `Restart=on-failure`) and by its own supervisor (`--background`) elsewhere: exit code 0
means the owner quit (or another copy was already running) and ends it; anything else restarts it
after 1, 2, 4, 8 and 16 seconds, giving up after five failures in two minutes.

## The editor and the browser

`screengrabcraft-editor` and `screengrabcraft-web` run the same `ui::Editor`; they differ only in
their `Services`: native file dialogs, files, the clipboard and the recent captures on the desktop;
the browser's file picker, downloads and local storage on the web.
