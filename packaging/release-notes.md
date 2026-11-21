## What's new in 0.1.56

New since 0.1.53:

- **Grab Text.** A new capture: drag over part of the screen (or click a window) and its text goes straight to the clipboard, with a notice showing how it begins. In the Image Editor, **Image › Grab Text**, the right-click menu or **Ctrl+Shift+T** reads the picture, or just the area picked with the Selection tool, into a window where the text can be put right before **Copy**. On the command line, `screengrabcraft-cli text shot.png` prints it. It uses Windows' own text reader on this computer, so nothing is sent anywhere. Windows only for now (greyed out on macOS and Linux). There's no shortcut by default; set one in Settings.
- **Zoom and pan in the Video Editor.** A new **Zoom and Pan** section in the Library zooms into part of the screen and back out. While a zoom is selected you see the whole screen, with the zoom's frame to drag and resize. Two zooms that touch pan straight from one to the other. The camera, titles and captions don't zoom.
- **Cut Out in the Image Editor.** A new tool next to Crop (key **U**): drag across the picture and that strip is taken out and the rest joined, with a straight, saw or wave edge.
- **A feature list** in the repository (`docs/parity.md`): every capture, picture editing, recording and video editing feature, what ScreenGrabCraft has, and the rest ranked by how much it would be missed.

**Early alpha.** The foundations are in place and tested, but much of what a daily screen capture tool needs is not there yet, and some platforms have only been checked by the build machines. What works is in the [README](https://github.com/GITHUB_REPOSITORY#readme).

ScreenGrabCraft for Windows, macOS, Linux, FreeBSD and the web. Every file is listed with its SHA-256 in `SHA256SUMS.txt`.

ScreenGrabCraft is three programs: the **capture service** (`screengrabcraft`), which lives in the system tray (the menu bar on macOS) and answers Print Screen and the other capture shortcuts; the **Image Editor** (`screengrabcraft-editor`), where captures and pictures are marked up, and the **Video Editor** (the same program in a window of its own), where Video Production recordings are cut and edited, with their transcript; and the command-line program (`screengrabcraft-cli`).

| Platform | File | |
|---|---|---|
| Windows 10/11 (x64, x86, ARM64) | `screengrabcraft-<ver>-windows-<arch>.msi` | Installer, just for you (no administrator: `%LocalAppData%\Programs\ScreenGrabCraft`; on the command line `msiexec /i <msi> ALLUSERS=2 MSIINSTALLPERUSER=1`) or for everyone on this PC (Program Files\ScreenGrabCraft, the default), with three Start menu entries: ScreenGrabCraft, ScreenGrabCraft Image Editor and ScreenGrabCraft Video Editor. The capture service starts at every sign-in (and right away from the finish page); turn that off in its settings or in Task Manager › Startup apps. Offers the Image Editor for `.png`, `.jpg`, `.gif`, `.bmp` and `.webp` without taking them over (choose it in Settings › Default apps); `.sgcraft` projects open in the Image Editor. Upgrading or uninstalling closes the running service. `screengrabcraft-cli.exe` is installed next to the programs. |
| | `screengrabcraft-<ver>-windows-<arch>-portable.zip` | Portable: unzip anywhere and run `screengrabcraft.exe` (or `screengrabcraft-editor.exe`); includes `screengrabcraft-cli.exe`. Registers nothing until you ask the service to start at sign-in. |
| macOS 11+ (Apple silicon and Intel) | `screengrabcraft-<ver>-macos-universal.dmg` | Drag both apps to Applications. ScreenGrabCraft runs in the menu bar (no Dock icon) and asks once for the Screen Recording permission; it starts itself at sign-in. |
| | `screengrabcraft-cli-<ver>-macos-universal.zip` | The command-line program alone. |
| Linux x86_64 and aarch64 (glibc 2.35+) | `screengrabcraft-<ver>-linux-<arch>.AppImage` | `chmod +x` and run (if it does not start, add `--appimage-extract-and-run`); it starts the capture service, and holds the editor and the CLI too. The `.zsync` next to it enables delta updates. |
| | `screengrabcraft-<ver>-linux-<arch>.deb` / `.rpm` | `sudo apt install ./screengrabcraft-*.deb` or `sudo dnf install ./screengrabcraft-*.rpm`. The capture service starts at every sign-in (`/etc/xdg/autostart`); a systemd user unit, `screengrabcraft.service`, is there as the alternative. |
| | `screengrabcraft-<ver>-linux-<arch>.flatpak` | `flatpak install --user screengrabcraft-*.flatpak` (the freedesktop runtime comes from Flathub). |
| | `screengrabcraft-<ver>-linux-<arch>.tar.gz` | `bin/` with the three programs; the desktop entries, metainfo, icons and the autostart entry under `share/`; the systemd user unit under `lib/`. |
| FreeBSD 15 (x86_64) | `screengrabcraft-<ver>-freebsd-x86_64.tar.gz` | The three programs, the desktop entries, the icons and the autostart entry. |
| Any modern browser | `screengrabcraft-web-<ver>.zip` | The editor as a static site (WebAssembly). It marks up pictures you open or paste; capturing the screen needs the desktop programs. |

On Linux the tray icon uses the StatusNotifierItem protocol: KDE Plasma, Xfce, Cinnamon and MATE show it; GNOME needs the AppIndicator extension. On Wayland the desktop's screenshot portal (`xdg-desktop-portal`) takes the pictures. Captures are saved in Pictures/ScreenGrabCraft.

The builds are not signed with a publisher certificate yet: Windows SmartScreen and macOS Gatekeeper will ask before running them.
