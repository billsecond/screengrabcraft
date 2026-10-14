# ScreenGrabCraft — instructions for agents

ScreenGrabCraft is a clean-room, open-source, Rust-native screen capture and annotation suite targeting Snagit parity: a capture service in the system tray and an annotation editor, on macOS, Windows, Linux and BSD, with the editor also on the web via WASM. It follows the conventions of its siblings, chiefly `../MailCraft` (Outlook) and the other Craft apps. Shared rules and learnings: [`storytold/craftrules`](https://github.com/storytold/craftrules) — read its `AGENTS.md`.

## Start every session here
1. Read `plan/STATUS.md` (current milestone, next task) if it exists. `plan/` is gitignored (local only); if it's missing, start from `ROADMAP.md`.
2. Don't stop to ask unless a decision is genuinely the owner's (licensing, publishing, pushing to new remotes, secrets, anything that touches the owner's own screen or files beyond a test).

## Never crash
A screen capture tool runs all the time; a crash loses a capture the owner can't take again. **This outranks feature work**: never ship a feature through a panic path, and fix a crash before building on top of it.
- **No panics in non-test code:** no `unwrap()`, `expect()`, `panic!`, `unreachable!`, `todo!`, `unimplemented!`; no `unsafe` (`unsafe_code = "forbid"`).
- **Errors are `Result<T, E>`** and `?`. An unfinished feature returns an error or is disabled; it never panics.
- **Everything from outside is hostile:** picture files, project files (`.sgcraft`), pasted data, command and CLI params, the X server's and the portal's replies. `get()` instead of `[i]`, checked/saturating arithmetic, clamp sizes (`screengrabcraft_model::raster::checked_len`, `MAX_PIXELS`), cap allocations and loops.
- **Last-resort guard:** `Session::execute` turns an escaped panic into an error and restores the document. It's a safety net, not a licence.
- **Prove it:** every crash fix lands with a regression test. `crates/engine/src/tests.rs::hostile_params_never_panic` runs every command with junk params — keep it green.
- Production crate roots carry `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]`; new crates start with it. Tests may `unwrap` (`clippy.toml`).

## Captures are private
- **No network code.** ScreenGrabCraft never sends anything anywhere: no telemetry, no accounts, no update checks, no upload. Sharing is the owner copying or saving a file. Adding a network feature is the owner's decision, not a convenience.
- **Captures stay where the owner put them:** the captures folder (`Pictures/ScreenGrabCraft` unless the settings say otherwise), the clipboard when asked, a temporary file when the editor needs one. Nothing else of a capture is written anywhere, and nothing of one is logged (sizes and file names only).
- **Screen recording permission is the system's:** on macOS the service asks Core Graphics and tells the owner where to allow it; on Wayland the portal decides. Never work around either.
- **Never test against the owner's own screen without saying so,** and never commit a capture of it. Tests use made-up pictures; screenshots in `docs/images` are of ScreenGrabCraft's own sample.

## Assets: absolutely no TechSmith (or Microsoft/Apple/Google) iconography or images
**This rule is non-negotiable.**
- Never copy, trace, screenshot-crop or recreate icons, images, stamps, themes, sounds, fonts or any other asset from Snagit or any other vendor's product. Not even "temporarily".
- Every asset must be **original** (drawn in code — prefer this: `crates/ui/src/icons.rs`, `crates/render/src/stamp.rs`, `crates/desktop/src/tray_icon.rs`, `xtask/src/art/`), **public domain / CC0**, **OSI open source**, or **Creative Commons that allows redistribution**.
- **Every asset file has a row in [`ATTRIBUTION.md`](ATTRIBUTION.md)** (author, source, licence). `cargo xtask assets` (part of `cargo xtask ci`) fails otherwise.
- The app icons are Craft-style engravings drawn in Rust (`xtask/src/art/`, `cargo xtask icon`): a falcon for the capture service, an owl with a pencil for the editor. See `assets/app-icon/README.md`.

## Clean room
Snagit may be **observed** black-box: run it, use it, note how it behaves. Never commit screenshots of it, never read, disassemble or copy anything inside it, never copy its wording beyond feature names. File formats come from public specs (PNG, JPEG, GIF89a, WebP, BMP, PDF 1.4).

## Architecture (see `docs/architecture.md`)
| Layer | Crate | Job |
|---|---|---|
| L1 | `model` | the document (picture + annotations), shapes, styles, geometry, settings |
| L2 | `render` | drawing annotations (tiny-skia), text (ab_glyph), effects, codecs, PDF, projects, stitching, GIFs |
| L3 | `capture` | monitors, windows and pixels: xcap (Windows, macOS), x11rb and the screenshot portal (Linux, BSD) |
| L4 | `engine` | Session, undo, **command registry** |
| L6 | `ui` | the egui editor and the capture overlay (builds for the browser) |
| L6 | `desktop` | settings files, logging, one instance, clipboard, the persistent service and its supervisor, launching |
| apps | `screengrabcraft`, `screengrabcraft-editor`, `screengrabcraft-cli`, `screengrabcraft-web` | binaries |

- **Everything is a command.** An editing action = a `CommandSpec` in `crates/engine/src/registry.rs` (id, label, group, shortcut, params doc, `enabled`, `run`) + tests. The editor's menus and keys, the CLI's `run` and scripts all dispatch by id. Interactive drags use `Session::begin`/`commit` so a drag is one undo step.
- **The engine never does I/O.** Files, the clipboard and dialogs are the host's (`crates/ui/src/services.rs`).
- **Layering** is enforced by `cargo xtask layers`. Nothing below L6 depends on egui/eframe/winit/rfd/tray-icon/global-hotkey.
- **Rust only** (no handwritten JS/TS, no Python). **Never break wasm** (`cargo xtask wasm`).

## The capture service
- One process: `apps/screengrabcraft`. eframe's root window is a 1×1 see-through anchor shown only while something is on screen (eframe opens windows only from a shown window, and paints a hidden one at 10 fps); the overlay, settings, countdown, notices and recording controls are immediate viewports.
- `--background` is the supervisor (restarts a crash with back-off; exit code 0 means stop); `--tray` runs the service itself; a second start hands its arguments to the first (`crates/desktop/src/instance.rs`).
- Sign-in registration is `crates/desktop/src/service.rs`; a build in `target/` never registers itself.

## Quality gates
Before every commit: `cargo xtask ci` (fmt, clippy -D warnings, tests, assets, layers, wasm). Commit after every arc of work that builds, with a task id in the message (`M1.2: scrolling capture`).

## Running and looking at the app
- `cargo run --release -p screengrabcraft` (the service; add `-- --capture region` to capture at once), `cargo run --release -p screengrabcraft-editor -- picture.png`.
- `SCREENGRABCRAFT_NO_PREFS=1` runs without settings, instance lock or sign-in registration (tests, CI).
- Logs: `<settings>/logs/service.log` and `editor.log` (`%APPDATA%\ScreenGrabCraft`, `~/Library/Application Support/ScreenGrabCraft`, `~/.config/screengrabcraft`).
- For UI work, look at the result: `cargo run -p screengrabcraft-render --example sample -- out.png` draws every annotation; the editor's behaviour is tested headless with egui_kittest (`crates/ui/src/tests.rs`).
- Parallel agents: separate `CARGO_TARGET_DIR` per agent; edit only the crates (or files) you own; format only your own files.
