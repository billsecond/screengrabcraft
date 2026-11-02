# ScreenGrabCraft

<img src="assets/app-icon/hicolor/128x128/apps/io.github.billsecond.screengrabcraft.png" width="96" alt="The ScreenGrabCraft icon: a selection being dragged out, with a cursor at its corner"> <img src="assets/app-icon/hicolor/128x128/apps/io.github.billsecond.screengrabcraft.editor.png" width="96" alt="The ScreenGrabCraft Editor icon: a barn owl with a pencil in its beak">

**An open-source screen capture and annotation suite, written from scratch in Rust.**

ScreenGrabCraft does what people use Snagit for: press Print Screen, drag out an area (or click a
window), and mark it up with arrows, callouts, numbered steps, blur and highlights before sharing
it. It is two programs:

- **ScreenGrabCraft**, the capture service. It lives in the system tray (the menu bar on macOS)
  and runs as a persistent per-user service: it starts when you sign in and is started again if
  it ever stops. It answers global shortcuts and the tray menu.
- **ScreenGrabCraft Editor**, where captures are marked up. It also runs in a browser.

Both are native, GPU-drawn programs (no Electron, no web view), with a command-line program
beside them for scripts and agents.

> **Status: early alpha (0.1).** Capturing, the editor and the packaging are in place and tested;
> see [ROADMAP.md](ROADMAP.md) for what's next and what has only been checked by the build machines.

| | |
|---|---|
| ![The editor: a picture with numbered steps, a box, a blurred area, an arrow and a selected callout, and the callout's properties](docs/images/editor.png) | ![The capture overlay: the frozen screen dimmed around the chosen area, its size, handles and the capture bar with Copy & Edit, Edit, Copy, Save and Cancel](docs/images/overlay.png) |

![Annotations drawn by ScreenGrabCraft: steps, a rectangle and an ellipse, a highlight, a blur and a pixelated area, a magnifier, stamps, a callout and arrows](docs/images/annotations.png)

## Capturing

| Shortcut (Windows, Linux) | macOS | |
|---|---|---|
| Print Screen | Ctrl+Shift+C | **Region**: drag out an area, or click a window; adjust it with its handles; Enter keeps it |
| Alt+Print Screen | Ctrl+Shift+W | **Window**: click a window |
| Ctrl+Print Screen | Ctrl+Shift+F | **Full screen**: the monitor under the pointer |
| — (set one in Settings; Ctrl+Shift+Print Screen is Windows' own) | — | **All screens** in one picture |
| Alt+Shift+Print Screen | Ctrl+Shift+S | **Scrolling**: scroll a long page while ScreenGrabCraft joins it into one picture |
| Ctrl+Alt+Print Screen | Ctrl+Shift+G | **Record GIF** of an area (press again, or click the tray icon, to stop) |
| Ctrl+Alt+Shift+Print Screen | Ctrl+Shift+V | **Record Video**: a small MP4 of an area, with the computer's sound (press again to stop) |
| — (set one in Settings) | Ctrl+Shift+P | **Video Production**: opens the Recorder; the screen, the camera, the microphone and the computer's sound, each on a track of its own, to edit |
| Shift+Print Screen | Ctrl+Shift+R | **Repeat** the last area |

Every shortcut can be changed (or switched off) in the service's settings, and the tray menu has
every capture, a delayed capture (3, 5 or 10 s), the editor and the captures folder. While choosing
an area the screen is frozen as it was when the key was pressed (open menus and tooltips included),
with a magnifier that shows the pixels, coordinates and colour under the pointer, crosshairs, the
windows under the pointer to click, and the area's size. Arrows nudge the area; Shift+arrows resize it.

As soon as you've chosen the area ScreenGrabCraft does its default: copy the capture to the
clipboard, open it in the editor, and save it in **Pictures/ScreenGrabCraft** (PNG, JPEG, WebP, BMP
or PDF, with a file name pattern of your own). The default is yours to change in the service's
settings. Turn the capture bar on to stop first, adjust the area, and pick Copy & Edit, Edit, Copy
or Save. An open editor is hidden while you capture, which you can turn off too.

A GIF recording runs at 30 frames a second unless you set it lower, is always saved, and opens in
the editor like any capture, with a **Play** button under the picture. Any GIF you open there has one. What you draw goes on a still of the first frame,
not on the animation, and the GIF file itself is never changed.

**Videos** are MP4 files with AV1 pictures and Opus sound: royalty-free, written in Rust, and
small (a minute of a screen with some typing is a few megabytes). Windows 11, macOS 14 and every
current browser play them; on Windows 10 the free AV1 Video Extension from the Microsoft Store
adds AV1. **Record Video** makes the MP4 as soon as you stop (no larger than 1920 × 1080 unless
you choose the full size). **Video Production** (the tray menu's video section, the Capture page,
or its shortcut) opens the **Recorder**. Choose the area: a **region** you drag out, a **window**
you click (where it was then; it isn't followed if it moves), one **full screen**, **all screens**
in one video (made no larger than 3840 × 2160), or a **fixed size** placed with a click: the usual
monitor and video sizes, landscape, portrait and square, as many as fit on your monitors, or your
own. Turn each input on or off: the screen,
the camera, the microphone (with a level meter once you turn it on or press Test) and the
computer's sound. With the screen off the camera's picture is the video. Press the red **Record**;
the Recorder remembers every choice. The recording file (`.sgvideo`) has a track for each source
and opens in the editor, where you cut and edit it, then Export Video makes the MP4 beside it. The
computer's sound can be recorded on Windows; the camera on Windows and macOS. Record Video's
sources are in Settings › Video.

## Editing a video

A Video Production recording opens in the editor as a timeline with a row for each track:

- **Play** (Space), click or drag on the ruler to move the playhead, arrows step a frame.
- Drag across empty rows to choose a stretch; **Delete** (or Cut) takes it out of every track,
  keeping them in step.
- Click a clip to select it: Delete removes just that clip (silence, or no camera, there). Drag a
  clip's ends to trim it, or the clip to slide it along its track.
- **S** splits at the playhead: the selected clip's track, or every track.
- Each sound track has **Mute** and a volume slider; the camera's corner, size and shape
  (square, rounded, circle) are on the right.
- Undo and redo everything. The edits are saved into the recording file a moment after each
  change (the recording itself is never changed), and File › Revert to Recording brings every
  track back whole.

## Editing

Annotations stay editable (move them, resize them, restyle them) until you flatten or export:

- **Arrow** (one or two heads, any thickness), **line**, **rectangle** (rounded corners),
  **ellipse**, **pen** and **highlighter**; Shift draws at 45° and makes squares and circles.
- **Text** and **callouts** (speech balloons with a tail you drag), in PatroneOS Sans or
  PatroneOS Mono, bold, italic, left, centred or right.
- **Step** numbers that count themselves, and **stamps** (check, cross, star, question, warning,
  information, heart).
- **Blur**, **pixelate**, **spotlight** (darken everything else) and **magnify** (a lens).
- Colours (a palette that reads well for people who see red and green alike, and any colour),
  thickness, opacity, shadows, dashes.
- **Crop**, **resize**, canvas size, rotate, flip, trim borders.
- **Effects**: border, drop shadow, torn edge, round corners, greyscale, invert, brightness and
  contrast.
- Everything stays on the picture: drawing, moving and resizing stop at its edge.
- Whatever tool is chosen, a click on an annotation selects it and a drag moves it (the pen and
  highlighter draw over things); its handles take hold even when the press is a little off.
- Rectangles, ellipses and callouts have a **fill** colour (none, the palette, or any colour).
- Undo and redo for everything, even after the editor is closed and the capture opened again
  (the last 30 steps are kept in its side file), copy and paste (a pasted picture lands as an editable object),
  drag and drop, zoom and pan, the strip of recent captures.
- Save as a **`.sgcraft` project** (JSON; the annotations stay editable) or export PNG, JPEG,
  WebP, BMP, GIF or PDF.

The editor's keyboard shortcuts are under Help › Keyboard Shortcuts.

## The command line

```
screengrabcraft-cli capture --all -o desktop.png
screengrabcraft-cli capture --window Notepad --clipboard
screengrabcraft-cli run shot.png --cmd 'object.add={"object":{"type":"arrow","from":{"x":20,"y":20},"to":{"x":200,"y":120}}}' --cmd 'effect.shadow' -o shot.sgcraft
screengrabcraft-cli render shot.sgcraft -o shot.png
screengrabcraft-cli commands
screengrabcraft-cli service status
```

The capture service takes requests too, which is how a capture is bound to a key on a Wayland
desktop (where programs can't take global shortcuts): `screengrabcraft --capture region`,
`--capture window|screen|all|repeat|scrolling|gif`, `--delay 5`, `--settings`, `--quit`.

## Platforms

| | |
|---|---|
| **Desktop** | Windows 10/11 (x64, x86, ARM64), macOS 11+ (Apple silicon and Intel), Linux (x86_64, aarch64; X11 and Wayland), FreeBSD (x86_64) |
| **Web** | the editor, in any modern browser (WebGPU, or WebGL2) |

How the service stays running everywhere:

| | Starts at sign-in | Restarted if it stops |
|---|---|---|
| Windows | the installer's `Run` entry (or your own, for the portable zip); switch it off in the service's settings or Task Manager › Startup apps | its own supervisor (`--background`) |
| macOS | a LaunchAgent the app registers on its first start from Applications | launchd (`KeepAlive`) |
| Linux, FreeBSD | the packages' XDG autostart entry (or your own); a systemd user unit is included as the alternative | its own supervisor, or systemd |

Screen capture goes through GDI on Windows, Core Graphics on macOS (allow ScreenGrabCraft under
Privacy & Security › Screen Recording), the X11 protocol on X11 and the desktop's screenshot portal
on Wayland. The tray uses the StatusNotifierItem protocol on Linux and the BSDs (KDE Plasma, Xfce,
Cinnamon and MATE show it; GNOME needs the AppIndicator extension).

## Installing

Releases are on the [Releases page](https://github.com/billsecond/screengrabcraft/releases) with
their checksums: an MSI and a portable zip for Windows, a disk image for macOS, AppImage, `.deb`,
`.rpm`, Flatpak and tarballs for Linux, a FreeBSD tarball and the web build. The builds are not
signed yet, so Windows and macOS ask before running them. See [docs/releasing.md](docs/releasing.md).

The Windows installer asks who it's for: **Just for me** installs into
`%LocalAppData%\Programs\ScreenGrabCraft` without an administrator; **Everyone on this PC** installs
into Program Files and Windows asks for an administrator's permission. An update keeps the choice.
On the command line, everyone on this PC is the default; just for you is:

```
msiexec /i screengrabcraft-<version>-windows-x64.msi ALLUSERS=2 MSIINSTALLPERUSER=1
```

(add `/passive` or `/qn` to install without the dialogs).

## Building

```
git clone https://github.com/billsecond/screengrabcraft
cd screengrabcraft
cargo run --release -p screengrabcraft          # the capture service
cargo run --release -p screengrabcraft-editor   # the editor
cargo xtask ci                                  # the checks every change must pass
```

Linux needs `libxkbcommon-dev libwayland-dev` to build; everything else (X11, D-Bus, the portal,
the tray) is spoken in Rust. The web build: `cd apps/screengrabcraft-web && trunk serve`.

## Privacy

Captures never leave your computer unless you send them somewhere. No accounts, no telemetry, no
upload. The one thing ScreenGrabCraft asks the network for is its own latest release on GitHub,
to tell you about an update (and on Windows to download and install it when you say so); that
request carries nothing about you or your captures, and "Check for updates automatically" turns it
off. See [PRIVACY.md](PRIVACY.md).

## Licence

MIT OR Apache-2.0, at your option. The bundled fonts are the
[PatroneOS fonts](https://patroneos.com/fonts) (PatroneOS Sans, PatroneOS Mono), under the SIL Open
Font Licence. Every asset is listed in [ATTRIBUTION.md](ATTRIBUTION.md); ScreenGrabCraft contains no
other vendor's icons or artwork, and is not affiliated with TechSmith or Snagit.
