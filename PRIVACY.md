# Privacy

ScreenGrabCraft takes pictures of your screen, so here is exactly what it does with them.

- **Nothing of yours leaves your computer.** No account, no telemetry, no crash reports, no
  upload. A capture goes where you send it — a file you save, the clipboard, another program you
  paste it into.
- **The one network request is the update check.** A minute after the capture service starts,
  and then every six hours, it asks GitHub for the latest release of ScreenGrabCraft
  (`https://api.github.com/repos/billsecond/screengrabcraft/releases/latest`). It is a plain
  HTTPS request that says only "ScreenGrabCraft" and its version: no identifier, no cookie,
  nothing about you or your captures. GitHub sees your IP address, as any website does. On
  Windows, when a newer version is out, its installer is downloaded from the same release into
  your temporary folder and checked against the release's checksums; it is installed only when
  you choose Update now. Elsewhere you are told, and the release page opens if you ask. Turn it
  off with "Check for updates automatically" (Settings › About); Check now still works when you
  ask. The Flatpak has no network access, so it never checks.
- **Where captures are kept:** in your captures folder (Pictures/ScreenGrabCraft unless you chose
  another), only when the "Save" setting is on or you press Save; on the clipboard when you copy;
  in a temporary file when a capture is opened in the editor without being saved.
- **What is written about them:** the log (in the settings folder) records a capture's size and
  file name, never its content. The settings remember the last area captured, for Repeat Last
  Capture.
- **Permissions:** on macOS ScreenGrabCraft asks for Screen Recording; on Wayland the desktop's
  screenshot portal may ask. ScreenGrabCraft never works around either.
- **The web editor** runs entirely in your browser; pictures you open stay there, and its settings
  are kept in the browser's local storage.

The source is open: <https://github.com/billsecond/screengrabcraft>.
