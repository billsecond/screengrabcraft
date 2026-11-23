## What's new in 0.1.57

New since 0.1.56: 100 more features from the feature list (`docs/parity.md`), and 9 more started. Six of them are on that list twice, once for recording or capturing and once for the editor, so here they share a line.

### Capturing

- **Freehand area:** draw round what you want with the button held down; what's outside is see-through.
- **Several areas in one capture:** hold Alt (Option on a Mac) as you let go of an area, then drag out the next.
- **Aspect ratio held while dragging:** Shift for a square, Ctrl (Cmd on a Mac) for 16:9, Ctrl+Shift for 4:3.
- **Areas of a set size:** a capture preset of a fixed size, placed with a click.
- **Open menus and tooltips** (Windows): an open menu, drop-down or tooltip is snapped to like a window, and a click takes just it.
- **Scrolling capture that scrolls by itself** (Windows): it turns the mouse wheel over the area and stops at the end of the page.
- **The mouse pointer as an annotation of its own:** when the settings put it in the picture, the editor can move, delete or restyle it.
- **Capture presets:** an area, a wait, effects added as it's taken (border, round corners, torn edge, shadow, grey), a folder, a file format and a shortcut, under one name; `screengrabcraft --preset <name>` for scripts.
- **Captures on a timer:** one every so many seconds, skipping those where nothing changed, or one at a time of day (the Capture page).
- **Record Steps:** a picture for each click, with a red ring where you clicked, made into a numbered how-to guide to reorder and word.
- **The guide saved** as a PDF, a word-processor document (`.docx`), slides (`.pptx`), a web page or Markdown.
- **A still from the camera** (Windows and macOS): Picture from the Camera, in the tray or on the Capture page.
- **QR codes read and made:** Image › Read QR Codes (a web address is shown, never opened) and Make a QR Code….

### Recording

- **Countdown before recording:** 3 seconds unless Settings › Recording says otherwise (1 to 10, or off).
- **Camera full screen:** F7, the recording's controls or the tray swaps the camera with the screen, and back; the video editor lists those stretches.
- **Camera border and mirror:** under the Recorder's camera picture; the video editor can change both.
- **Keys pressed shown in the video** (Windows): the shortcuts you press, never what you type; the video editor's Keys Pressed section sets where, how large, and light or dark keys.
- **A recording cut short by a crash is kept:** it's moved to the captures folder as "Recording cut short …".

### Image Editor: drawing

- **Curved arrows:** drag the diamond in an arrow's middle; heads notched, solid, an open V, a dot or a bar.
- **Polygons, triangles, diamonds and stars:** 3 to 12 corners or points with the Shape tool, or a polygon clicked point by point.
- **Callouts with several tails:** up to 8, each dragged on its own (Add a Tail).
- **Step badges as letters or Roman numerals:** A, a, I or i, in a circle, rounded square, square, diamond, hexagon or pin.
- **A stamp library:** 34 stamps drawn in code (25 symbols and 9 faces), with a search, and pictures of your own.
- **Any font on the computer** for text and callouts, with a search (not in the browser).
- **Spell check:** unknown words get a red wavy line as you type a text or a callout; right-click for corrections or Add to Dictionary. US and UK spellings are both known.
- **Gradient fills:** linear at any angle or radial, for shapes, callouts, text boxes and step badges.
- **Eraser** (E): rubs the picture out to see-through or to a colour.
- **Group, align, distribute and flip annotations** (Ctrl+G groups), and select all of one kind.
- **Rotate annotations:** the round handle over a box or a group; Shift for 15° steps.
- **Rulers and guides:** View › Rulers and Guides; what you move snaps to them.
- **Measure tool** (I): a drag shows the length and the angle; keep it on the picture as a dimension line.
- **Your own tool bar:** names on or off, and the tools you want (View › Customize Tool Bar…).
- **Themes of your own,** made from the colours in use, in the theme menu.

### Image Editor: selecting and changing the picture

- **Ellipse and freehand selections, and Invert** (Ctrl+Shift+I); moving, cropping and the colour effects keep to the shape.
- **Magic wand:** selects by colour, with a tolerance.
- **Lift a part and move it:** the Selection tool's "Fill where a moved part was" fills the gap.
- **Simplify:** Image › Simplify draws the screen's parts as plain shapes and its text as grey bars.
- **Rotate by any angle:** Image › Rotate and Flip › Rotate by an Angle…, the annotations turned with it.
- **Templates:** Image › Template… lays pictures out in a row, down the page, in a grid, or before and after, with a title and words.
- **Pictures combined into one:** Image › Combine Pictures…, side by side, stacked or in a grid.

### Image Editor: effects

- **Effect presets:** eight ready-made (Soft Shadow, Photo Frame, Rounded Card, Torn Paper, Old Photo, Faded Edges, Thin Line, Black and White), and your own with Keep as Preset….
- **Effects on a selected area only:** the colour changes, sharpen and blur.
- **Wavy, faded and bevelled edges,** beside the torn edge.
- **Hue, saturation and gamma,** with lightness, in one effect.
- **Sepia, sharpen and blur.**
- **Replace a colour:** Effects › Replace a Colour…, picked with the eyedropper, with a tolerance.
- **Perspective, page curl and reflection.**
- **Watermark:** a picture or words, in a corner, the middle or all over.

### Keeping and sharing pictures

- **Batch Changes:** resize, effects, grayscale, invert and trim many pictures at once, with a format and a name pattern; nothing is written over.
- **Pin captures** to the front of the Recent strip (right-click › Pin to the Front).
- **One PDF of several captures,** a page each, from All captures or Batch Changes.

### Video Editor: the timeline

- **A video made from pictures:** File › New Video from Pictures, to narrate over.
- **Lock a track, or play one alone:** the padlock and S in its header.
- **J, K and L:** play backwards, pause, play on; pressed again, 2, 4 and 8 times as fast.
- **Push later clips along:** Shift-drag a clip.
- **Join pieces back together** from a clip's or a track's right button.
- **Hold a frame** (H): every track waits while the picture is held.
- **Speed up or slow down a stretch:** a quarter as fast to eight times, voices keeping their pitch.
- **Play a clip backwards,** picture and sound.
- **Line up clips by their sound:** Line Up by Sound With, on a sound clip's right button.
- **Groups of clips** across tracks, moved together.
- **Markers** (M), which become the MP4's chapters.
- **Clip colours:** eight, from a clip's right button.
- **Replace a clip and keep its effects; copy and paste effects.**
- **Subtitle files brought in:** File › Import Subtitles, SubRip (`.srt`) or WebVTT (`.vtt`).
- **Turn an item; crop a clip.**
- **Corner pin:** a picture or a video pinned onto a slanted surface by its four corners.

### Video Editor: motion and effects

- **Zoom on the Clicks:** zooms that follow the clicks by themselves.
- **Keyframes:** place, size and opacity over time, eased from one to the next.
- **Motion along a curved path** through the keyframes.
- **Transitions:** a cross-fade, through black, wipes four ways, a push and a circle; sound clips cross-fade.
- **Words that write themselves on:** Write on, as a title's, a lower third's or a callout's entrance or exit.
- **Camera background taken away without a green screen,** then your room blurred, the screen, or a picture behind you.
- **Green screen:** a colour taken away, and its glow on you.
- **Colour adjustments and eight looks:** brightness, contrast, colour and tint; warm, cool, vivid, faded, dramatic, black and white, sepia and night.
- **Border, shadow and rounded corners** on a clip, and the screen made smaller on a background colour.
- **Device frames, masks and blend modes:** a plain laptop, phone or browser window drawn round the screen; masks in six shapes; multiply, screen, overlay, darken and lighten.
- **Motion blur, and a region of the screen frozen** while the rest plays on.
- **Pointer highlight, spotlight, magnifier and click sounds,** in the Pointer panel.
- **The pointer hidden for a stretch, given another look, or its path edited.**
- **Templates and themes of your own** in the Library's Yours, used on other recordings.

### Video Editor: sound and words

- **A volume line along a clip:** double-click a clip's wave to add a point.
- **Fade in and out.**
- **Background noise taken out,** learnt from the track itself, on this computer.
- **Even Out the Levels, and music lowered under the voice.**
- **A video's sound separated from its picture; Mix to Mono.**
- **Narration recorded over the video** in the side panel's Narration (desktop only).
- **Captions edited** in the transcript pane: a word put right, split, joined or moved.

### Video Editor: making the file

- **Just the sound:** File › Export Sound, WAV everywhere, M4A on Windows.
- **A frame saved as a picture,** which opens in the Image Editor.
- **Just the chosen stretch,** with its captions and chapters.
- **See-through video:** a MOV of PNG pictures, clear wherever there's nothing.
- **A watermark,** words or a picture, in Export Video's window.
- **Several recordings made at once:** File › Make Videos of Several Recordings.

### Started, more to come

- **Drawing on the screen while recording:** a pen, arrows, boxes and step numbers (F8); Windows only, and not on a recording of every screen at once.
- **Personal details found and hidden:** Image › Hide Personal Details… covers email addresses, phone numbers, card numbers and address lines; Windows only, and faces aren't found.
- **Edit text in the picture:** Image › Edit Text in the Picture… puts new text in place of a line, in its size and colour; Windows only, and the font isn't matched yet.
- **Capture details on the picture:** the time, what it's of and its size; the program's name is typed for now.
- **Print:** File › Print or Ctrl+P, on Windows only.
- **Captures sorted by program, tags and favourites** in All captures; websites only by their page's title.
- **TIFF** opened and saved; HEIF not yet.
- **Captions that light up word by word:** View › Light Up Each Word as It's Said; each word's time is worked out from its line, not heard.
- **Cut words by deleting them from the transcript, and Take Out Filler Words;** a word's time is worked out from its line, not heard.

### Fixed

- **The editor never opens as a tiny window stuck at the top left of the screen.**

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
