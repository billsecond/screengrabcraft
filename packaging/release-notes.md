**Early alpha.** The foundations are in place and tested, but much of what a daily screen capture tool needs is not there yet, and some platforms have only been checked by the build machines. What works is in the [README](https://github.com/GITHUB_REPOSITORY#readme).

ScreenGrabCraft for Windows, macOS, Linux, FreeBSD and the web. Every file is listed with its SHA-256 in `SHA256SUMS.txt`.

ScreenGrabCraft is three programs: the **capture service** (`screengrabcraft`), which lives in the system tray (the menu bar on macOS) and answers Print Screen and the other capture shortcuts; the **editor** (`screengrabcraft-editor`), where captures and pictures are marked up; and the command-line program (`screengrabcraft-cli`).

| Platform | File | |
|---|---|---|
| Windows 10/11 (x64, x86, ARM64) | `screengrabcraft-<ver>-windows-<arch>.msi` | Installer for all users: Program Files\ScreenGrabCraft and two Start menu entries, ScreenGrabCraft and ScreenGrabCraft Editor. The capture service starts at every sign-in (and right away from the finish page); turn that off in its settings or in Task Manager › Startup apps. Offers the editor for `.png`, `.jpg`, `.gif`, `.bmp` and `.webp` without taking them over (choose it in Settings › Default apps); `.sgcraft` projects open in the editor. Upgrading or uninstalling closes the running service. `screengrabcraft-cli.exe` is installed next to the programs. |
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
