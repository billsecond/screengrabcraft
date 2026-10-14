# Privacy

ScreenGrabCraft takes pictures of your screen, so here is exactly what it does with them.

- **Nothing leaves your computer.** ScreenGrabCraft has no network code: no account, no
  telemetry, no crash reports, no update checks, no upload. A capture goes where you send it —
  a file you save, the clipboard, another program you paste it into.
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
