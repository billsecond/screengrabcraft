# ScreenGrabCraft roadmap

**Stage: early alpha** · next: alpha that has been used every day on all three desktop systems

> **Last updated:** 2026-11-01 · **Target:** TechSmith Snagit (capture and editor), cross-platform

ScreenGrabCraft aims at the parts of Snagit people use every day, everywhere, and then goes
further on openness, privacy and scripting.

## Where it stands (2026-10-14)

| | State |
|---|---|
| Capture: region, window, full screen, all screens, repeat, delayed | done; Windows tried by hand; macOS, X11 and Wayland built and unit-tested, X11 and the Wayland portal exercised under Xvfb, Xephyr and sway |
| Capture: scrolling (manual scroll, joined), GIF recording | done; the joining is unit-tested, the recordings not yet tried by hand on every system |
| Overlay: frozen screen, magnifier, crosshair, window snapping, handles, capture bar | done; tested headless and on Windows |
| Persistent service: tray, global shortcuts, sign-in start, supervisor | done on Windows, macOS and Linux/BSD; Windows tried by hand |
| Editor: 15 annotation kinds, styles, crop, resize, effects, undo, projects, export | done; tested headless and on Windows |
| Command line | done |
| Web editor | builds; not yet tried in every browser |
| Packages: MSI, portable zip, DMG, AppImage, deb, rpm, Flatpak, FreeBSD, web | written and statically checked; first release builds them on GitHub |

## Next

1. Use it daily on macOS and on Linux (GNOME with the AppIndicator extension, KDE) and fix what
   that turns up, especially the overlay's placement on mixed-DPI desktops. On macOS the overlay
   is a floating window, so the menu bar and the Dock stay above it (an area under them can't be
   dragged from there); on Wayland, where programs can't list windows, a click picks the whole
   screen rather than a window.
2. Video recording (MP4) next to GIF; a GIF editor (trim, speed).
3. Automatic scrolling for scrolling captures (scroll the window under the area for the owner).
4. Text recognition (copy the text in a capture), without a network service.
5. Templates and saved styles ("themes") in the editor; a library view of all captures with
   search and tags.
6. Global shortcuts on Wayland through the GlobalShortcuts portal.
7. Signed builds (Windows Authenticode, Apple Developer ID and notarisation).
8. A lighter idle service: on Windows it holds about 200 MB (DirectX 12) or 145 MB (OpenGL) of
   working set while waiting in the tray, nearly all of it the graphics driver; release the GPU
   while nothing is on screen.

## Progress log

| Date | What landed |
|---|---|
| 2026-10-14 | First version: the six crates and four programs, Craft-style icons drawn in Rust (a falcon for the capture service, an owl with a pencil for the editor), packaging and release workflows for Windows, macOS, Linux, FreeBSD and the web |
| 2026-10-14 | 0.1.1: the capture overlay no longer takes the service down on Windows (DirectX 12 can't make a surface for a window created full screen and always-on-top at once, so it's raised after its first frame); a service that dies while opening a window draws with OpenGL from its next start; graphics errors are logged instead of ending the program |
| 2026-10-31 | New app icons, flat pictures that read at every size: a selection and a cursor for the capture service, a selection and a pencil for the editor, in place of the engraved falcon and owl. |
| 2026-10-31 | The capture bar is hard to miss: big buttons with words and their keys, Edit and Copy in the overlay's bright colour, in an outlined panel centred under the area; it no longer panics on a screen narrower than it |
| 2026-11-01 | The default runs at once: choosing an area copies the capture, opens it in the editor and saves it, all of it set in the settings, and the capture bar is now the setting that stops first (with the default as a button of its own). An open editor is hidden before any capture and comes back after it, however it ends, even if the service goes down |
| 2026-11-01 | The PatroneOS fonts (1.1.0) replace Inter and JetBrains Mono, in the interface and in annotations. They cover the Latin letters of most European languages; annotation text in anything else (Greek, Cyrillic, Vietnamese, symbols like ✓) draws as empty boxes, as there's no fallback font |
| 2026-11-01 | GIFs play: a recording opens in the editor like any capture, and any animated GIF there has a Play button. It's read a frame at a time, so a long recording holds one frame in memory; the GIF file is never changed |
