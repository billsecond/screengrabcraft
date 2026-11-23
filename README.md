# ScreenGrabCraft

<img src="assets/app-icon/hicolor/128x128/apps/io.github.billsecond.screengrabcraft.png" width="96" alt="The ScreenGrabCraft icon: a selection being dragged out, with a cursor at its corner"> <img src="assets/app-icon/hicolor/128x128/apps/io.github.billsecond.screengrabcraft.editor.png" width="96" alt="The ScreenGrabCraft Image Editor icon: a barn owl with a pencil in its beak">

**An open-source screen capture and annotation suite, written from scratch in Rust.**

ScreenGrabCraft does what people use a screen capture tool for: press Print Screen, drag out an area (or click a
window), and mark it up with arrows, callouts, numbered steps, blur and highlights before sharing
it. It is two programs:

- **ScreenGrabCraft**, the capture service. It lives in the system tray (the menu bar on macOS)
  and runs as a persistent per-user service: it starts when you sign in and is started again if
  it ever stops. It answers global shortcuts and the tray menu.
- **ScreenGrabCraft Image Editor**, where screenshots and pictures are marked up (it also runs in
  a browser), and the **Video Editor**, the same program in a window of its own, where recordings
  are cut and edited. Each one's Recent strip lists its own: pictures in one, recordings in the
  other.

Both are native, GPU-drawn programs (no Electron, no web view), with a command-line program
beside them for scripts and agents.

> **Status: early alpha (0.1).** Capturing, the editor and the packaging are in place and tested;
> see [ROADMAP.md](ROADMAP.md) for what's next and what has only been checked by the build machines.

![The editor: a capture of a shop's dashboard marked up with numbered steps, a box round one number, a highlighted total, a blurred column of addresses, an arrow and a selected callout, with the callout's quick styles and properties at the right](docs/images/editor.png)

| Capture | Edit a video |
|---|---|
| ![The capture overlay: the frozen screen dimmed around the chosen area, its size, handles and the capture bar with Copy & Edit, Edit, Copy, Save and Cancel](docs/images/overlay.png) | ![The video editor: a recording with a name put on it and the camera in its corner, the library at the left, and a timeline with the screen, the computer's sound, the microphone, the camera and the captions, a stretch chosen with the handles on the ruler](docs/images/video-editor.png) |

These are ScreenGrabCraft's own windows, running on Windows, each put on a made-up desktop. The
program being captured and edited in them is made up, and so are its customers and the person on
the camera (`cargo run -p screengrabcraft-ui --example shots -- docs/images dark live` opens each
window for a few seconds and keeps a picture of it, and of nothing else on the screen).

## Capturing

| Shortcut (Windows, Linux) | macOS | |
|---|---|---|
| Print Screen | Ctrl+Shift+C | **Region**: drag out an area, or click a window; adjust it with its handles; Enter keeps it |
| — (set one in Settings) | — | **Region, adjust first**: drag out an area, or start from the last one, and adjust it with its handles until Enter (or a button of the bar) takes it |
| — (set one in Settings) | — | **Freehand area**: draw round what to keep with the button held down; what's outside the outline is see-through |
| Alt+Print Screen | Ctrl+Shift+W | **Window**: click a window |
| Ctrl+Print Screen | Ctrl+Shift+F | **Full screen**: the monitor under the pointer |
| — (set one in Settings; Ctrl+Shift+Print Screen is Windows' own) | — | **All screens** in one picture |
| Alt+Shift+Print Screen | Ctrl+Shift+S | **Scrolling**: a long page joined into one picture; on Windows it scrolls the page by itself and stops at its end (elsewhere, or with that setting off, you scroll it) |
| Ctrl+Alt+Print Screen | Ctrl+Shift+G | **Record GIF** of an area (press again, or click the tray icon, to stop) |
| Ctrl+Alt+Shift+Print Screen | Ctrl+Shift+V | **Record Video**: a small MP4 of an area, with the computer's sound (press again to stop) |
| — (set one in Settings) | Ctrl+Shift+P | **Video Production**: opens the Recorder; the screen, the camera, the microphone and the computer's sound, each on a track of its own, to edit |
| Shift+Print Screen | Ctrl+Shift+R | **Repeat** the last area |
| — (set one in Settings) | — | **Grab Text**: drag out an area (or click a window) and its text goes on the clipboard (Windows) |
| — (set one in Settings) | — | **Picture from the Camera**: a still from Video Production's camera, at its own size (Windows, macOS) |

Every shortcut can be changed (or switched off) in the service's settings, and the tray menu has
every capture, a delayed capture (3, 5 or 10 s), the editor and the captures folder. While choosing
an area the screen is frozen as it was when the key was pressed (open menus and tooltips included),
with a magnifier that shows the pixels, coordinates and colour under the pointer, crosshairs, the
windows under the pointer to click (on Windows an open menu, drop-down or tooltip too, to keep just it),
and the area's size. Arrows nudge the area (Ctrl+arrows, Cmd on a Mac, ten pixels at a time);
Shift+arrows resize it. A recording's frame (a GIF, a scrolling capture or a video) has a round
grip with arrows four ways in its middle: drag it to move the whole frame, its size kept, anywhere
on its monitor before the recording starts.
Hold Shift while dragging for a square, Ctrl (Cmd on a Mac) for 16:9, both for 4:3. Hold Alt
(Option on a Mac) as you let go to keep that area and drag out another: the last one without Alt
(or Enter) keeps them all in one picture, laid out as they were on the screen with the rest
see-through (Backspace takes the last one out again).
With "Include the mouse pointer in captures" on, the pointer is drawn into the capture, and in the
editor it is an annotation of its own, to move or delete.

As soon as you've chosen the area ScreenGrabCraft does its default: copy the capture to the
clipboard, open it in the editor, and save it in **Pictures/ScreenGrabCraft** (PNG, JPEG, WebP, BMP
or PDF, with a file name pattern of your own). The default is yours to change in the service's
settings. Turn the capture bar on to stop first, adjust the area, and pick Copy & Edit, Edit, Copy
or Save, for every capture; or use **Capture Region, Adjust First** (tray menu, or a shortcut of your own)
for just the captures you want to fine-tune. An open editor is hidden while you capture, which you can turn
off too (the video editor never is: it stays as you left it).

**Capture presets** (Settings › Capture presets) are captures of your own making: a region, a region
held to a shape (16:9, square…) or of a set size placed with a click, a freehand area, a window, the screen, all
screens or a scrolling capture; a wait first; what happens after (copy, edit, save, the bar); effects
added as it's taken (a border, round corners, a torn edge, a shadow, grey); a folder and a file format
of its own (its captures are in the editor's Recent strip and All captures with the rest, even after
the preset changes folder); and a shortcut of its own. They're on the Capture page and in the tray menu (Capture
Presets), and `screengrabcraft --preset <name>` takes one from a script or a desktop's own shortcut.

**Captures on a timer** (the Capture page, or Captures on a Timer in the tray): one every so many
seconds of the screen with the pointer, all screens or the last area, saved straight to the
captures folder; one where nothing has changed since the last is skipped. Stop them from the tray.
"One at" takes a single capture at a time of day.

**How-to guides** (Record Steps, on the Capture page or in the tray): while it runs, each click takes
a picture of the window clicked with a red ring where the click was (a small bar at the top counts
the steps and stops it). Stop, and the guide window lists the numbered steps to put in order, take
out and word, and saves the guide as a PDF, a word-processor document (`.docx`), slides (`.pptx`, a slide
a step), a web page or Markdown (the last two with the pictures in a folder beside them). The guide
itself is a `.sgguide` file in the captures folder (`screengrabcraft --guide <file>`
opens it again).

A recording (a GIF, a video, Video Production) starts after a **countdown** shown over the area, 3 seconds
by default; Settings › Recording changes it or turns it off.

**While you record**, you can still take pictures: Capture Region, Window, Full Screen, All Screens
or Repeat, by their shortcuts or the recording's tray menu (Take a Screenshot). The recording goes
on; on Windows the screen frozen to choose the area, and what's said once it's kept, stay out of
the video. The picture is saved and copied as usual, and opens in the editor once the recording
stops, so nothing comes up over the screen being recorded.

**Draw on the screen while you record** (Windows): press F8, or the pen on the recording's controls,
and draw over the area with a pen, arrows, boxes or step numbers. They go into the recording and
fade away after a few seconds; F8, Done or Esc stops drawing. (Not on a recording of every screen
at once.)

On Windows the editor is kept ready in the background (Settings, After a capture): it starts a
little after the capture service, and closing its window hides it, so a capture opens in it at
once instead of waiting for Windows to start it. The editor opens where you left it: the same
place and monitor, the same size, maximized or not.

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
computer's sound. With the screen off the camera's picture is the video. Under the camera's picture,
**Mirror** flips it left to right and **Border** puts a line of your colour round it (the video
editor can change both after). While it records, the camera button on the controls (or F7, or the tray)
puts the camera full screen with the screen in its corner, and back. Press the red **Record**;
the Recorder remembers every choice. The recording file (`.sgvideo`) has a track for each source
and opens in the video editor, where you cut and edit it, then Export Video makes the MP4 beside it. The
computer's sound can be recorded on Windows; the camera on Windows and macOS. Record Video's
sources are in Settings › Recording. If ScreenGrabCraft or the computer goes down while it records, what was
recorded is kept: the next start says so, and Edit opens it in the video editor.

On Windows a Video Production recording also keeps **the shortcuts you press** while it records,
so the video editor can show them as keys on the video: only a key pressed with Ctrl, Alt or the
Windows key, and keys that don't type (Enter, Esc, Tab, Backspace, Delete, the arrows, Home, End,
Page Up and Down, F1 to F24). **Letters, digits, symbols and spaces you type are never kept**, nor
anything typed with AltGr, so a password typed while recording isn't in the recording. The keys
are only watched while recording, kept in the recording file and nowhere else, and never logged.
Settings › Recording turns it off.

![The Recorder: the screen, the camera with its picture, the microphone and the computer's sound with their levels, each switched on, and the red Record button](docs/images/recorder.png)

## Editing a video

Recordings open in the **video editor**: a window of its own beside the picture editor, with its
own taskbar button and the same look (the tray menu, the Start menu and Settings open it too).
Pictures you take while you edit a video open in the picture editor, so the video stays as it
is; a picture opened in the video editor goes to the picture editor, and a recording opened in
the picture editor to the video editor. Its timeline has a row for each track:

- **Play** (Space), click or drag on the ruler to move the playhead, arrows step a frame. The
  full-screen button is beside the play buttons (F11 or F; Esc comes back).
- **Choose a stretch** with the two handles either side of the playhead on the ruler: they ride
  along with it until you drag one out. With a stretch chosen, Play plays just that: from the
  playhead when it's in it, else from the stretch's start, stopping at its end. Dragging across
  empty rows chooses one too, Ctrl+A chooses everything, Esc lets go. **Delete** (or Ctrl+X) takes
  the stretch out of every track, keeping them in step.
- **Right-click a track** to split that track alone, take the chosen stretch out of it alone
  (leaving silence, or no picture, so everything stays in time), choose the clip's stretch,
  delete the clip, or hide or mute the track. **Right-click the ruler** for every track at once:
  split them all, cut or remove the chosen stretch, choose from there to the end or the start,
  add text there, play, zoom.
- Click a clip to select it: Delete removes just that clip (silence, or no camera, there). Drag a
  clip's ends to trim it, or the clip to slide it along its track.
- **S** splits at the playhead: the selected clip's track, or every track.
- **Speed**: with a stretch chosen or a clip selected, the panel at the right (or Edit, or the
  ruler's right button) plays it from a quarter as fast to eight times as fast, on every track so
  they stay together; what follows moves up to follow on. Voices keep their pitch, and a clip
  that isn't as recorded says its speed on the timeline.
- **J, K and L**: L plays on, K pauses, J plays backwards; press J or L again to play that way
  2, 4 or 8 times as fast (playing on, voices keep their pitch; backwards its sound is turned
  round, and faster backwards it plays silent). **Play backwards**: a clip's right button (or Play backwards in its panel) plays it from its end to
  its start, the picture and the sound turned round, with the clips grouped with it.
- **Hold a frame**: H (or Edit, or the ruler's right button) holds the picture at the playhead
  still for a while (2 s; the menus offer 1 to 5): every track waits, the sound is silent
  meanwhile, and what follows moves along. Its panel holds it longer or shorter (or drag its end),
  or takes the hold out again.
- **Fades and noise**: click a sound clip and its panel fades its sound in at its start and out at
  its end, and takes its track's **background noise** out (a fan, a hum, hiss: learnt from the
  track itself, on this computer; voices stay). The track's right button turns the noise off and
  on too.
- **Groups, locks and colours**: a clip's right button groups it with the other tracks' clips over
  the same time (they then move together) and gives it a colour. **Shift-drag** a clip to push
  every later clip of its track along with it. The padlock in a track's header **locks** it (its
  clips stay as they are; cuts of the whole video still keep it in time), and **S** plays a sound
  track alone.
- **Volume line and levels**: double-click a sound clip's wave (or use the track's right button) to
  put a point of its volume line there, and drag points up and down or along: the sound goes
  straight from one to the next, and the points stay with the sound through every edit. The Sound
  Mix section of the panel **evens out the levels** (each sound track's volume set so they're about
  as loud, measured on this computer) and **lowers the rest under the voice**: the computer's
  sound, music and media go down while the microphone is heard, and back up after.
- **Motion along a path**: with two keyframes or more, an item's Motion has Along a curved path:
  it goes through their places on a smooth curve without stopping at them. While it's chosen,
  its way shows on the picture as a dashed line with a dot at each keyframe.
- **Corner pin**: a picture's or a video's panel has Corner Pin. Its four round handles are then
  dragged anywhere on the picture, and it's drawn in perspective between them (a screen seen at an
  angle, a poster on a wall).
- **Words that write themselves on**: a title's, a lower third's or a callout's Entrance (or Exit)
  can be Write on: its words appear letter by letter from the first, as if written (and are rubbed
  off the same way going). Anything without words is wiped on from its left.
- **Freeze a region**: Edit › Freeze a Region (or the screen track's right button, with a stretch
  chosen) holds a rectangle of the screen as it was while the rest plays on: a clock, a message
  that popped up. Drag it and its corners on the picture; its panel sizes it too.
- **Replace media, copy effects**: an item's right button (or its panel's Effects) has Replace
  With, to show another file of the Media bin in its place, keeping its time, place, look, motion
  and animations. Copy Effects and Paste Effects put one item's look, entrance, exit and motion on
  another, or one clip's colours and crop (or its fades) and the transition into it on another.
- **Join pieces back together**: a clip's right button joins it with the next one when that goes
  on from where it stops (pieces of a split, not moved apart); a track's right button joins all
  its pieces, and the ruler's every track's.
- **Line up by sound**: a sound clip's or a sound item's right button (or the item's panel) has
  Line Up by Sound With, listing the other sounds: it moves so the two sound in step, for the same
  voice recorded twice (by the computer and by another recorder). They're compared on this
  computer, at every shift within 5 seconds.
- **A video's sound apart, and mono**: a video put on a layer has Separate the Sound in its panel
  (or its right button): its sound goes onto a sound layer of its own at the same moment and the
  video goes silent, so each can be moved, trimmed or deleted on its own. **Mix to Mono** (a sound
  track's right button or its clip's panel, a sound layer's right button or its item's panel) mixes
  its two sides into one, heard the same from both speakers.
- **Markers**: M (or Edit, or the ruler's right button) marks the moment at the playhead with a
  flag on the ruler. The Markers section of the panel names them, goes to them and deletes them;
  they move with cuts and speed changes, and Export Video makes them the MP4's chapters, which
  video players list.
- **Export more than the video**: with a stretch chosen, Export Video can make **just that
  stretch** (its captions and chapters with it). File › **Export Sound** makes just the sound, as
  heard: a WAV, or a smaller M4A (AAC, made by Windows' own encoder, so on Windows only). Export
  Video's **See-through video** makes a MOV of PNG pictures, clear wherever there's nothing (hide
  the screen's track to keep just the camera without its background, titles and callouts), to put
  over another video; it has no sound. File › **Make Videos of Several Recordings** makes each
  recording chosen into a video as it was edited, beside it, with one setting for all. File ›
  **Save the Frame as a Picture** (Ctrl+Shift+S, or the ruler's right button) saves the picture at
  the playhead, at the recording's own size, in the captures folder and opens it in the picture
  editor.
- **A watermark**: Export Video's window puts words, or a picture from the Media Bin, on every
  frame, in the corner you choose, as see-through and as large as you like. The preview shows it
  too, and it stays with the edits.
- **Subtitles brought in**: File › **Import Subtitles** reads a SubRip (`.srt`) or WebVTT
  (`.vtt`) file timed on the video as it is now and makes it the video's captions; from then on
  each line follows the track it's on, like the captions written down here.
- **The pointer**, in a recording that has it: the Pointer section of the panel at the right shows
  it larger or smoother, rings its clicks and lets them be heard (a click made in code), and puts a
  yellow highlight under it, a spotlight round it (the rest dimmed) or a lens on it that shows
  what's round it twice as large. With a stretch chosen, Hide It in the Chosen Stretch (or the
  screen track's right button) hides it there, clicks and all; a dashed line on the screen's row
  shows where, and it stays hidden over what the screen showed there through later edits. Its
  Pointer list draws another in its place: a white or black arrow, a hand, a dot or a ring. Edit
  Its Path in the Chosen Stretch puts points along its way there on the picture (while the
  playhead is in the stretch): drag them where it should have gone, and it glides through them;
  Straighten makes it go straight from the first to the last.
- **The camera's background taken away**, so the screen shows round you (the camera's panel,
  Background): **Remove a colour** for a green screen (choose the colour and how far from it
  still goes; its green glow on you is taken out), or **Remove it** with no green screen: the
  background model (the one Remove Background uses, on this computer) looks at four of the
  camera's pictures and learns what the room behind you looks like, then every picture keeps
  what differs from it. Keep the camera still; Learn It Again if the light changed. What it
  learnt is kept in the recording file. **Behind you** then says what shows where it was taken
  away: the screen, your room blurred, or a picture you brought into the Media bin.
- **Keys pressed**: in a recording that kept the shortcuts pressed (see above), the Keys Pressed
  section of the panel shows each as keys on the video for a moment after it's pressed, where the
  edits put that moment: at the bottom (above the captions), the top or a corner, any size, light
  or dark keys.
- **Zoom and pan**: the Library's Zoom and Pan section zooms into part of the screen and back. While
  a zoom is chosen the picture shows all of the screen and the zoom's frame: drag the frame where it
  should zoom and its corners to zoom closer. It goes in and out smoothly or at once, and a zoom that
  starts where another ends pans straight from one to the other. Only the screen and the pointer
  are zoomed; the camera, titles and captions stay where they are. **Zoom on the Clicks** (at the
  top of that section, or the Edit menu) makes a zoom for each run of clicks the recording kept:
  in a little before the first, out a while after the last, panning between ones close together.
  They're ordinary zooms, to move, change or delete.
- **Pictures in turn**: the Media bin's Put the Pictures in Turn places every picture there one
  after another from the playhead, a few seconds each (as many as asked), each as large as fits
  the video, fading from one to the next. Hide the screen's track for a video of just them, and
  talk over them with Narration.
- **A video of pictures alone**: File › New Video from Pictures (or A Video of Pictures on the
  start page) asks for pictures and makes a new video of just them: a blank screen as long as they
  take (1920 × 1080, saved in your captures folder like any recording), each picture four seconds,
  one after another. Talk over it with Record Narration, then make the video as any other.
- **Narration**: the side panel's Record Narration turns the microphone on (the one Video
  Production records with, or another from its list) and plays the video from the playhead
  without its sound while you speak. Pause (Space), Stop Narration or the video's end ends it: the
  take is saved as a WAV beside the recording and goes on a sound layer of its own, Narration, from
  where it began. The microphone is on only while it records.
- **Motion blur**: the side panel's Motion Blur (or a zoom's panel) blurs what moves along its way:
  items sliding, growing or moving by their keyframes, and zooms going in, out or panning. Each
  such moment is drawn five times over the shutter and blended, so it takes longer to make.
- **Transitions between clips**: where two clips of a track meet, one can turn into the other (a
  cross-fade, through black, a wipe four ways, a push or a circle), from the clip's panel, its
  right button, or the Library's Between Clips section dropped on the cut. It plays across the
  cut, 0.5 to 3 s; where sound clips meet it's a cross-fade of their sound.
- **Turned and framed**: an item's panel turns it (Turn); a picture's or a video's crops its
  sides, puts a border of any colour round it, rounds its corners and gives it a soft shadow
  (Frame). A screen or camera clip's panel makes it lighter or darker, changes its contrast and
  colour (down to grey), and crops its edges (what's left fills the picture, its shape kept).
- **Looks, masks and blends**: a screen or camera clip's panel gives it a ready-made look (warm,
  cool, vivid, faded, dramatic, black and white, sepia, night) or a tint of any colour. An item's
  panel (Mask and Blend) shows it only inside a shape (a rectangle, rounded rectangle, oval,
  diamond, star or heart), or cuts the shape out of it, and lays it on what's under it as
  multiply, screen, overlay, darken or lighten.
- **Screen frame and devices**: the side panel's Screen Frame makes the recording's screen smaller
  on a background colour, rounds its corners and gives it a border and a shadow, and can draw a
  plain laptop, phone or browser window round it. A picture or a video on a layer can have one
  round it too (its panel, Device round it). The devices are simple shapes drawn in code.
- **Keyframes**: an item's panel adds a keyframe at the playhead (Motion); move the playhead on and
  move, resize or fade the item (on the picture or in the panel) and it goes there smoothly. A
  zoom's keyframes pan it along. The timeline shows each as a diamond on the item.
- **Text** (T) puts words on the video at the playhead, ready to type over. The **Library** at
  the left has titles, lower thirds, news graphics, callouts, arrows, a countdown, an end card, a
  checklist, a click and a **blur** that hides what's under it (a password, an address), and
  transitions, to drag onto the timeline; and a bin for your own pictures, videos and sound. Each
  card shows the item as it will be on the video, where and as large; hold the pointer on one and
  it plays, larger.
- **Templates and themes of your own**: the Library's Yours section keeps what's on the layers
  (or just the item chosen) under a name, look and all, kept in the settings folder; its card
  drops it on another recording, each item where it was from the template's start. Themes gives
  the titles, lower thirds and callouts colours and a face (the program's sans, slanted, or fixed
  width): a few come with it, Save as Yours keeps your own, Apply to This Video restyles what's
  there, and what's added from the Library comes in it. A text item's panel changes its face too.
- Each track and layer has a switch to **hide or mute** it, and arrows to **move it up or down**
  (a layer higher up is in front); **double-click its name to rename it** (or use the right
  button). Each sound track has a volume slider; the camera's corner, size and shape (square,
  rounded, circle), its border (how wide, its colour) and Mirror are on the right, with the
  stretches the camera fills the picture: choose a stretch and Camera Full Screen for the Stretch,
  or Remove one.
- **Transcript** (the tool bar, or View) opens a pane at the right with everything said, line by
  line: when, who and what. Words from the microphone are **You**, from the computer's sound
  **Others**, and from a sound or video you added, its layer's name; rename any of them (the pencil,
  or a double click) and the name is the track's or the layer's, in the transcript, on the
  timeline and in Export Transcript. A click on a line takes the playhead there; while the video
  plays, the line playing is marked and kept in view. Copy puts the whole transcript on the
  clipboard. **Put a line right**: double-click it to change its words (and split it at the
  cursor); its right button splits it, joins it to the speaker's next line, moves its start or end
  to the playhead, or deletes it. The lines put right are what the picture, Export Video and the
  subtitles show; All Captions as Heard takes them back. **Cut words out of the video**: a line's
  right button, Cut Words from the Video, shows its words: click those to take out and press
  Delete, and where they're said goes from every track. **Take Out Filler Words** (in the pane,
  or Edit) cuts every um, uh and er the transcript has. Each word's time is worked out from its
  line's by its length, so check the cut by playing it (undo puts it back).
- Undo and redo everything. The edits are saved into the recording file a moment after each
  change (the recording itself is never changed), and File › Revert to Recording brings every
  track back whole.
- **Captions**: after a Video Production recording, what you said (the microphone, "You") and
  what the others said (the computer's sound, "Others") is written down on this computer, in
  the background: one thread at the lowest priority, never while recording, about a twelfth of
  the recording's length (Whisper base, English; nothing is sent anywhere). They appear on their
  own row under the tracks and follow your edits. **Show** puts them on the picture, **In video**
  burns them into the MP4, and an `.srt` and a `.txt` are saved beside the recording (and beside
  each MP4 made, as it was edited). When you use speakers rather than headphones, the others
  heard again by your microphone aren't written down twice. Turn them off with Captions in the
  Recorder; File › Make Captions makes them for a recording that has none. View › **Light Up Each
  Word as It's Said** lights each word in turn on the picture and in the video made (when each
  word is said is worked out from its line's time by its length).

## Editing

Annotations stay editable (move them, resize them, restyle them) until you flatten or export:

![Annotations drawn by ScreenGrabCraft: steps, a rectangle and an ellipse, a highlight, a blur and a pixelated area, a magnifier, stamps, a callout and arrows](docs/images/annotations.png)

- The tool bar has **Favorites**, **Arrow**, **Text**, **Callout**, **Shape**, **Stamp**,
  **Fill** and **Move** in the middle, then every other tool, each with its own full-size button
  and its name under it (a narrow window gives the tools a row of their own; View › Tool Names
  turns the names off). View › Customize Tool Bar… shows the tools you choose, in your order.
  Every tool keeps its key, on the bar or not.
- **Quick Styles**: ready-made looks for each tool in a theme of four colours (Bold, Soft, Vivid,
  Mono), drawn as they will look. A click sets the tool's look and changes the selected
  annotations to it; the star on a look keeps it under **Favorites**, with the favourites of every
  tool, and a look made by hand can be kept too. Themes of your own: "New theme from the current
  colours…" in the theme menu makes one of the colours in use, named and kept with the four. See
  [the editor's tools](docs/editor-tools.md).
- **Grab Text** (Image menu, the right-click menu, Ctrl+Shift+T): the text in the picture, or in the
  area selected, read on this computer by Windows' own text reader and shown to copy (put anything
  right first). On macOS and Linux it isn't there yet.
- **QR codes** (Image menu): **Read QR Codes** shows the words in the QR codes in the picture (or
  the area selected), each to copy; a web address is shown, never opened. **Make a QR Code…** puts
  a code of your words or web address on the picture, to move and size like any picture. Both on
  this computer, everywhere.
- **Hide Personal Details** (Image menu, the right-click menu): email addresses, phone numbers,
  card numbers and address lines found by the same reader, each covered with a blur, a pixelate
  or a solid box that stays an annotation to move or delete (Windows; faces aren't found).
- **Edit Text in the Picture** (Image menu, the right-click menu): the lines of text found are
  outlined; click one and it's covered with the colour round it and a text box takes its place in
  the same size and colour, ready to type into (Windows; set in the computer's interface font).
- **Eyedropper** in every colour menu: pick any colour from the picture. Over the picture the
  eyedropper and the Fill tool's pail are the pointer, their tip on the spot they act on.
- **Selection** (Q): select part of the picture itself to move, copy, cut, delete or crop to: a
  box, an ellipse, a freehand outline, or a colour with the magic wand; Invert Selection
  (Ctrl+Shift+I) takes everything else. What is done to a selected area is also under Image ›
  Selected Area. **Eraser** (E): rubs the picture out to see-through or
  to a colour. With "Fill where a moved part was" ticked, a part dragged away (a button, a block)
  leaves its place filled from the colours round it; **Fill from Round It** takes something out
  of the picture that way.
- **Simplify** (Image menu): the picture's parts drawn over it as plain shapes, flat boxes in
  their colours and grey bars for the text, one group under the other annotations (Ungroup to
  change one).
  **Flatten** the selected annotations or all of them; a **right-click menu** on the picture has
  these and the rest. Copy with annotations selected copies just them.
- **Arrow** (one or two heads, notched, solid, an open V, a dot or a bar, any thickness; straight
  or curved: drag the diamond in a selected arrow's middle to bend it), **line**, **shape** (a
  rectangle, rounded or not, an ellipse, a triangle, a diamond, a polygon of up to 12 corners or a
  star, picked in its Shape options, or a polygon of points clicked one by one: Points clicked, then
  a double click or Enter closes it) and **pen**; Shift draws at 45° and makes squares and circles.
  A line bends as an arrow does, by the diamond in its middle.
- **Highlighter** (H): drag a box over what should stand out; it's tinted as under a marker
  (dark stays dark), and resized by its handles or moved like any shape.
- **Rulers and guides** (View menu): rulers along the top and the left; drag a guide out of one,
  drag it to move it, back onto its ruler to take it away. What is moved snaps its edges and
  middle to the guides and to the picture's edges and middle (View › Snap to Guides).
- **Measure** (I): drag across the picture for a distance in pixels, across, down and at what
  angle; keep it on the picture as a dimension line with its length, or keep every one.
- **Hand** (D): drag the picture round in the window. Space held down, or the middle button,
  does the same with any tool.
- **Text** and **callouts** in five forms (a rounded box, a box, an oval, a cloud and an arrow
  with the text in it) with a tail set to point down left, down right, left, right, up or down,
  and dragged anywhere, and more tails (Add a Tail, up to 8) to point at several things; in
  PatroneOS Sans or PatroneOS Mono or any font on the computer (Font,
  with a search; a project keeps the font's name, and shows the built-in font where it's missing;
  not in the browser), bold, italic, left, centred or right.
  Text overlays: an outline round the letters, a shadow, a filled or see-through box behind
  them, and big titles, each a quick style of the Text tool. Spelling is checked as you type
  (English, US and UK spellings): unknown words get a red wavy line, and a right click on one
  offers corrections and Add to Dictionary; Edit › Check Spelling turns it off.
- **Step** badges that count themselves, in numbers, capital or small letters, or Roman numerals
  (each way counts on its own, so 1, 2, 3 can have a, b, c beside them), on a circle, a rounded
  or square box, a diamond, a hexagon or a pin (Shape, in the Step tool's properties), and
  **stamps**: a library of 34 drawn in code (check, cross, star, arrows, a pin, a lock, a light
  bulb, faces and more) with a search, and pictures of your own (a logo, a signature) kept as
  stamps from a file or the clipboard.
- **Blur**, **pixelate**, **spotlight** (darken everything else) and **magnify** (a lens).
- Colours (a palette that reads well for people who see red and green alike, and any colour),
  thickness, opacity, shadows, dashes.
- **Fill** (F): click the picture to fill an area of one colour (a tolerance takes in colours
  close to it; none makes it see-through), or click a box, ellipse, callout or text box to fill it;
  a linear or radial gradient to a second colour, too.
- **Crop**, **resize**, canvas size, rotate (a quarter turn, or any angle with Image › Rotate and
  Flip › Rotate by an Angle…: the corners see-through or a colour, the annotations turned with it), flip, trim borders.
- **Cut Out** (U): drag across the picture over what to take out (columns, or rows) and the rest
  is joined, straight or with a saw or a wave and a thin see-through gap. Annotations past the strip
  move with the picture. An area chosen with the Selection tool can be cut out from its right-click
  menu too.
- **Properties and Effects, docked at the right**: Properties (the quick styles and the tool
  properties) over Effects (brightness and contrast; hue, saturation and gamma; a colour
  replaced by another, picked from the picture with the eyedropper, everywhere or only where it
  joins the point picked; sharpen; blur;
  border, drop shadow, round corners; torn, wavy and faded edges, a bevel; perspective, a page
  curl, a reflection; a watermark of words or a picture, over the picture or under its words and
  lines; the capture's details written on it; each opened with its settings and Apply;
  greyscale, sepia and invert under Colours). **Presets** at the top apply an effect, or a few one
  after another (Photo Frame, Rounded Card, Torn Paper, Old Photo...), with one click; Keep as
  Preset… under an effect keeps its settings under a name of your own. The colour changes, sharpen and blur keep to an
  area selected on the picture. Drag the panes' left edge to make them wider, and the bar
  between them to give either more room (a double click puts it back). The Effects menu has
  them too.
- **Combine Pictures** and **Template** (Image menu): this picture with recent captures or picture
  files, side by side, stacked or in a grid; or laid out on a page with a title, step numbers and
  each step's words, every part an annotation to change.
- **Turn, group and line up annotations**: the round handle over a selected box (or group) turns
  it, Shift in steps of 15°; Edit › Group (Ctrl+G) and Ungroup, Rotate and Flip, Align and
  Distribute, and Select All of This Kind, in the right-click menu too.
- Everything stays on the picture: drawing, moving and resizing stop at its edge.
- Whatever drawing tool is chosen, a click on an annotation selects it and a drag moves it (the
  pen draws over things); its handles take hold even when the press is a little off.
- Rectangles, ellipses and callouts have a **fill** colour (none, the palette, or any colour), or a
  **gradient**: linear at any angle, or radial, to a second colour (the Fill menu).
- Undo and redo for everything, even after the editor is closed and the capture opened again
  (the last 30 steps are kept in its side file), copy and paste (a pasted picture lands as an editable object),
  drag and drop, zoom and pan, the strip of recent captures. Right-click a capture there and Pin to
  the Front keeps it first in the strip (remembered with its tags in `library.json`).
- **All captures** (in the strip): every capture by day, with a search through names, tags,
  programs and window titles. A star makes a capture a favourite; its tag button (or a right click
  in the strip) gives it tags. Filters show only the favourites, the captures taken from one
  program, or those with one tag. The program and tags are kept in `library.json` in the settings
  folder, never in the pictures.
- Save as a **`.sgcraft` project** (JSON; the annotations stay editable) or export PNG, JPEG,
  WebP, BMP, TIFF, GIF or PDF. TIFF files open too.
- **Print** (File › Print, Ctrl+P) through Windows' own printing window (greyed out elsewhere).
- **Open With…** (the File menu for the picture as shown, or a capture's right-click menu in the
  strip for its file): Windows' own window for choosing a program to open it in (greyed out
  elsewhere).
- **Batch Changes** (File menu, or Batch Change These in All captures): the same changes (resize,
  border, shadow, torn edge, round corners, brightness and contrast, grayscale, invert, trim) made
  to many pictures at once, each saved beside its picture under a name pattern (`{name} changed`).
- **A PDF of several captures**: in All captures, PDF of These makes one PDF of the pictures shown
  (or of those chosen with Ctrl+click), a page each, oldest first; or choose "One PDF, a page each"
  as Batch Changes' format. Where it goes is asked.

The editor's keyboard shortcuts are under Help › Keyboard Shortcuts.

## The command line

```
screengrabcraft-cli capture --all -o desktop.png
screengrabcraft-cli capture --window Notepad --clipboard
screengrabcraft-cli run shot.png --cmd 'object.add={"object":{"type":"arrow","from":{"x":20,"y":20},"to":{"x":200,"y":120}}}' --cmd 'effect.shadow' -o shot.sgcraft
screengrabcraft-cli run shot.png --picture logo.png --cmd 'effect.watermark={"picture":0,"place":"bottomRight"}' -o marked.png
screengrabcraft-cli render shot.sgcraft -o shot.png
screengrabcraft-cli text shot.png
screengrabcraft-cli commands
screengrabcraft-cli service status
```

The capture service takes requests too, which is how a capture is bound to a key on a Wayland
desktop (where programs can't take global shortcuts): `screengrabcraft --capture region`,
`--capture adjust|window|screen|all|repeat|scrolling|gif|text`, a capture preset by its name or number
(`--preset "Docs shot"` or `--capture preset:2`), `--delay 5`, `--settings`, `--quit`.

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
other vendor's icons or artwork, and is not affiliated with the maker of any other capture or video tool.
