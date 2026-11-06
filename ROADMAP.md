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
| 2026-11-01 | Everything in the editor stays on the picture: nothing starts on the gray around it, drawing, moving, resizing and the arrow keys stop at its edge, and what a click, a paste or a duplicate places, or typing grows, is kept on it. Panning and zooming work anywhere |
| 2026-11-01 | Smooth GIF recordings: 30 frames a second by default, up from the 5 or 6 a 12 setting really gave on a 4K monitor. On Windows a frame copies only the area, frames hold only what changed, and encoding runs on threads of its own, so it can't hold up the capture |
| 2026-11-02 | Captures can be deleted from the editor's strip (a bin button on hover, the open one too); they go to the Recycle Bin (the Trash), never for good |
| 2026-11-02 | Edits save themselves: a moment after each change the picture file is written again and its side file (`.sgcraft`) keeps the original and the annotations, so File › Revert to Original goes back. No "Save your changes?" prompts on the desktop |
| 2026-11-02 | The editor remembers the last tool, its tool bar buttons are a third bigger, the strip's pictures a quarter bigger with more room, and the picture is fitted and centred again (never past 100%) when the window or the picture changes size |
| 2026-11-02 | Dialogs, notices and the settings window look like the system's: a 1 px border, a title with an X, and the buttons at the bottom right, the default one first and filled ([Cancel] [OK] on macOS) |
| 2026-11-02 | Automatic updates: the service checks GitHub's latest release a minute after it starts and every six hours (the one network request; it can be turned off). On Windows the new installer is downloaded, checked against the release's checksums and installed when you choose Update now |
| 2026-11-02 | Video: Record Video (Ctrl+Alt+Shift+Print Screen) makes a small MP4 (AV1 and Opus, royalty-free, written in Rust) as soon as it stops; Video Production records the screen, the computer's sound, the microphone and the camera on tracks of their own, and the editor cuts, splits, trims, mutes and places the camera before Export Video makes the MP4. The computer's sound on Windows; the camera on Windows and macOS |
| 2026-11-02 | 0.1.9: Record GIF is on Ctrl+Alt+Print Screen and Scrolling Capture on Alt+Shift+Print Screen (they swapped), and Capture All Screens has no shortcut by default, as Windows uses Ctrl+Shift+Print Screen itself. A settings file still on the old defaults is brought up to the new ones once; shortcuts chosen by hand stay |
| 2026-11-02 | The editor: roomier menus; a click on an annotation selects it and a drag moves it whatever the tool, and handles take hold within 13 points; the fill colour of boxes, ellipses and callouts out in the open; the last 30 undo and redo steps kept in the side file (project format 3), so Undo works after the editor is closed and the capture opened again |
| 2026-11-02 | 0.1.12 (0.1.10 and 0.1.11 were tagged, but their builds stopped at faulty checks of the new per-user install and nothing was published): a Recorder window for Video Production (the screen, the camera, the microphone and the computer's sound, each on or off with its device, and the area: a region, a window, a full screen, all screens or a fixed size), opened from its own section of the tray menu; the recording control is never in the video (it goes beside the area, or on a screen that isn't recorded, or stays hidden and the tray icon stops it); cancelling the area goes back to the Recorder |
| 2026-11-03 | 0.1.13: Stop and Cancel always work, even when the camera sends no pictures (a device gets two seconds to stop, then is left to stop on its own; the control says Stopping at once, and Cancel closes it at once; a camera that has sent nothing for five seconds is reported on the control); the Recorder's fixed sizes are the usual landscape, portrait and square monitor and video sizes, as many as fit on the monitors |
| 2026-11-03 | 0.1.14: a Fill tool in the editor (F, and the Image menu): a click fills the area of the picture round it that is all one colour, within a tolerance, or fills the box, ellipse, callout, text box, step or stamp clicked; None makes the area see-through; one undo step each. The view no longer zooms back to fit when only the picture's pixels change, and the tool bar's buttons get smaller in a narrow window rather than run into the zoom |
| 2026-11-06 | 0.1.15: captures are quicker. On Windows the editor is kept ready out of sight (Windows took seconds over its first start), a closed editor window only hides, and a capture is handed straight to the running editor; the frozen screen goes as soon as the area is chosen. The editor opens where it was left (place, monitor, size, maximized). Capture Region, Adjust First: adjust the area (starting from the last one) before it's taken. The logs are timed to the millisecond and keep their history |
