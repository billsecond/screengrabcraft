# Feature parity

What a full screen capture, picture editing, screen recording and video editing suite does, and
where ScreenGrabCraft stands on each. The list was made from public product pages, help articles,
tutorials and release notes, noting what each feature does and nothing else; the wording here is
our own.

**Has** means it's there and works. **Part** means some of it is there (the note says what's
missing). **Missing** means it isn't there yet. **Not planned** means it needs an online service
or an account, which ScreenGrabCraft doesn't have: captures stay on this computer
([PRIVACY.md](../PRIVACY.md)), so any of these is the owner's decision, not a gap to fill.

The gaps are ranked at the end, by how much someone using it every day would miss them.

## Taking pictures

| Feature | Status | Notes |
|---|---|---|
| Region: drag out an area | Has | |
| Window: click a window | Has | |
| Full screen; all screens in one picture | Has | |
| Repeat the last area | Has | |
| Delay before the capture | Has | 3, 5 or 10 seconds from the tray |
| Frozen screen while choosing, with a magnifier, crosshairs and the pixel's colour | Has | |
| Arrow keys nudge the area, Shift+arrows resize it | Has | |
| Windows snap under the pointer | Has | |
| Open menus and tooltips caught as they were | Has | the screen is frozen when the key is pressed, so open menus are in it; on Windows an open menu, drop-down (the system's, or the program in front's own) or tooltip is snapped to like a window, a click keeping just it; elsewhere it's dragged round |
| Parts of a window (a button, a pane) snapped to | Missing | |
| Freehand-shaped area | Has | Capture Freehand Area (the tray, the Capture page, a shortcut or a preset): drawn round with the button held down; what's outside the outline is see-through |
| Several areas in one capture | Has | Alt (Option on a Mac) held as an area is let go keeps it and another is dragged out; the areas of one screen in one picture, laid out as they were, the rest see-through |
| Fixed size or fixed place area | Has | a capture preset of a set size, placed with a click; the Recorder's fixed sizes for videos |
| Aspect ratio held while dragging (square, 16:9, 4:3) | Has | Shift, Ctrl, Ctrl+Shift while dragging (the overlay says so); a preset can hold any shape |
| The program's own windows kept out of the picture | Has | the editor steps aside |
| The mouse pointer in the picture | Has | drawn into the picture when Settings say so, and kept in the capture's side file as an annotation of its own, so the editor can move or delete it |
| **Grab Text: the text in an area straight to the clipboard** | Has (0.1.54) | Windows' own text reader, on this computer; Windows only for now |
| Scrolling capture, scrolled by hand | Has | |
| **Scrolling capture that scrolls by itself** | Has | Windows: it turns the mouse wheel over the area and stops at the end of the page or at the longest picture; elsewhere, by hand |
| Captures again and again on a timer (time-lapse), or at a set time | Has | the Capture page: one every so many seconds of the screen with the pointer, all screens or the last area, skipping those where nothing changed; one at a time of day; saved straight to the captures folder, stopped from the tray |
| A picture for each click, made into a numbered how-to guide | Has | Record Steps (the Capture page or the tray): each left click takes the window clicked with a red ring where the click was; Stop opens the guide, its steps to reorder, take out and word |
| The guide saved as a document, slides, PDF, web page or Markdown | Has | a PDF (a page a step), a word-processor document (`.docx`) and slides (`.pptx`, a slide a step) with the pictures in them, a web page and Markdown with the pictures in a folder beside them |
| A still from the camera | Has | Picture from the Camera (the tray, the Capture page, a shortcut, after a wait if asked): Video Production's camera, read at its largest and kept at its own size (up to 4096 on a side); Windows and macOS |
| A whole web page from its address | Missing | |
| Printing from any program into the editor | Missing | |
| HDR screens captured in their true colours | Missing | |
| QR codes made or read | Has | the Image menu: Read QR Codes (the picture or the area selected) shows each code's words to copy, a web address shown and never opened; Make a QR Code… puts one of your words on the picture as a picture annotation. On this computer, every system |
| Pictures sent from a phone | Not planned | needs a network service |
| Capture presets (an area type, effects and where it goes, under one shortcut) | Has | the area (any kind, a shape or a set size), a wait, what happens after, effects added as it's taken (border, round corners, torn edge, shadow, grey), a folder and a file format of its own (its captures listed in the Recent strip and All captures with the rest, the folders remembered) and a shortcut of its own; on the Capture page, in the tray, and `screengrabcraft --preset <name>` for scripts and a desktop's own shortcuts |
| After a capture: copy, open in the editor, save, or ask with a bar | Has | |
| Every shortcut changed or turned off | Has | |

## Recording the screen

| Feature | Status | Notes |
|---|---|---|
| Region, window, full screen, all screens | Has | |
| Fixed sizes: landscape, portrait, square, your own | Has | |
| Frame rate, quality, scaled to 1080p | Has | |
| A smaller file format | Has | AV1 and Opus, beside H.264 and AAC on Windows |
| Microphone and the computer's sound | Has | the computer's sound on Windows only |
| **Several microphones and cameras at once** | Has | the + beside the Recorder's Microphone and Camera switches: up to four of each, each on a track of its own (each camera in a helper program of its own), named by its device in the video editor; each muted, moved, trimmed and soloed on its own, the sounds mixed (the rest lowered under any of the voices), each camera in a corner of its own, dragged anywhere on the picture. The first camera's background and full screen only; captions of the first microphone only |
| Camera in a corner: square, rounded, circle | Has | |
| Camera full screen, swapped with the screen while recording | Has | the camera button on the recording's controls, F7 or the tray: the camera fills the picture, the screen in its corner, and back; kept as stretches the video editor's camera panel lists, adds to and removes (`video.cameraFull`). With the screen off the camera is the video |
| Camera border, mirrored camera | Has | Mirror and Border (with its colour) under the Recorder's camera picture, shown there as they will be; the video editor's camera panel changes both, and how wide the border is |
| **Camera background blurred or replaced** | Has | taken away in the video editor (a colour, or the background learnt by the model), then the camera panel's Behind you: the screen, your room blurred, or a picture of the Media bin as large as covers it |
| Each source on a track of its own, to edit | Has | |
| Pause, resume, stop by keys (F9, F10); restart; discard | Has | |
| Recording controls kept out of the video | Has | |
| Countdown before recording | Has | 3 seconds by default (Settings › Recording: 1 to 10, or off), shown over the area and gone before the recording starts; a GIF, a video, Video Production |
| Longest recording | Has | Settings |
| Microphone heard as silent (muted or not allowed) said plainly | Has | after the recording |
| Background noise taken out | Has | in the video editor, after recording: a track at a time |
| Pointer: where it is, clicks, which pointer | Has | |
| Pointer made larger, smoothed, click rings, hidden | Has | |
| Pointer highlighted, spotlit or magnified; click sounds | Has | in the video editor's Pointer panel |
| **Keys pressed shown in the video** | Has | Video Production keeps the shortcuts pressed (Windows; never what's typed), the video editor shows them |
| Drawing on the screen while recording | Part | a pen, arrows, boxes and step numbers, drawn into the recording and fading after a few seconds (F8, the controls or the tray); Windows only, and not on a recording of every screen at once |
| A recording survives a crash | Has | the recording file is written as it goes; when the capture service starts again after a crash, it keeps what was recorded (Record Video's moved from the temporary folder to the captures folder as "Recording cut short …") and says so, Edit opening it in the video editor |
| Desktop icons hidden while recording | Missing | |
| **A narrated video made from pictures** | Has | File › New Video from Pictures (or the start page's A Video of Pictures): a new recording of nothing but a blank screen as long as the pictures take (1920 × 1080, an ordinary `.sgvideo` in the captures folder), the pictures one after another on a layer over it, four seconds each, as large as fits, fading from one to the next; then Record Narration talks over it. The Media bin's Put the Pictures in Turn does the same over a recording |

## Editing a picture

| Feature | Status | Notes |
|---|---|---|
| Arrow: one or two heads, thickness, dashes, shadow | Has | |
| **Curved arrow, more head shapes** | Has | a diamond in a selected arrow's middle bends it, or Straight and Curved in its Shape options; heads notched, solid, an open V, a dot or a bar |
| Line, pen, highlighter | Has | a line bends as an arrow does: the diamond in its middle, or Curved in its Shape options |
| Text: fill, outline round the letters, shadow, alignment | Has | |
| **Any font on the computer** | Has | for text and callouts, with a search; used, never shipped; a project keeps the font's name and shows the built-in font where it's missing; not in the browser |
| Callouts in five forms, tails any way | Has | |
| **Callout with several tails** | Has | Add a Tail in a selected callout's Shape options or its right-click menu: up to 8, each dragged on its own, in every form |
| Rectangle, rounded rectangle, ellipse | Has | |
| **Polygon, triangle, star** | Has | triangles, diamonds, polygons of 3 to 12 corners and stars of 3 to 12 points, drawn in a box with the Shape tool; and polygons of points clicked one by one (Shape options › Points clicked: a double click or Enter closes it), moved and resized as a box |
| **Stamps: a library with a search, faces, pictures of your own** | Has | 34 drawn in code (25 symbols and 9 faces); pictures of your own kept in the settings folder (not in the browser) |
| Fill an area or a shape | Has | |
| **Gradient fills** | Has | linear at any angle or radial, from the fill colour to a second one: shapes, callouts, text boxes and step badges (the Fill menu), and the Fill tool on the picture or on a shape; a gradient quick style for shapes |
| Move, select, nudge, duplicate | Has | |
| Selection tool: move, copy, cut, delete or crop to an area | Has | |
| **Ellipse, polygon or freehand selection; invert** | Has | the Selection tool's Box, Ellipse and Freehand (any outline, a polygon drawn round); Invert (Ctrl+Shift+I); moving, copying, deleting, cropping and the colour effects keep to the shape |
| **Magic wand (select by colour)** | Has | with a tolerance, joined to the pixel clicked or anywhere |
| Crop, trim borders | Has | |
| **Cut Out: take a strip out and close the gap** | Has (0.1.56) | columns or rows; straight, saw or wave |
| Blur, pixelate | Has | |
| **Personal details found and hidden by themselves** | Part | Image › Hide Personal Details…: email addresses, phone numbers, card numbers (Luhn's check) and address lines, found by Grab Text's reader and covered with a blur, a pixelate or a solid box, each an annotation to move or delete; Windows only; faces aren't found (no face model with a clear open licence) |
| **Simplify (screen parts drawn as plain shapes)** | Has | Image › Simplify: areas of one colour become flat boxes in their colour, icons small boxes, lines of text grey bars (from Grab Text's lines on Windows, from the marks themselves elsewhere); one group under the other annotations |
| **Lift a button or a block and move it, filling where it was** | Has | the Selection tool's "Fill where a moved part was", and Fill from Round It to take something out |
| **Edit text in the picture, in a matching font** | Part | Image › Edit Text in the Picture…: the lines Grab Text finds are outlined, and a click on one covers it with the colour round it and puts a text in its place in the same size and colour, to type into; Windows only; the font itself isn't recognised (the computer's interface font is used) |
| Step numbers that count themselves | Has | |
| **Steps as letters or Roman numerals; other badge shapes** | Has | A, B, C, a, b, c, I, II, III, i, ii, iii, each counting on its own; circle, rounded square, square, diamond, hexagon, pin |
| **Eraser** | Has | a tool of its own (E): rubs the picture out to see-through, or to a colour, at a width |
| Magnify, spotlight | Has | |
| Eyedropper, any colour | Has | |
| Remove background | Has | a model on this computer |
| Make a colour see-through | Has | |
| **Grab Text from the picture or an area** | Has (0.1.54) | Image menu, right-click, Ctrl+Shift+T |
| **The captured pointer moved or deleted** | Has | the capture service keeps the pointer as a picture annotation in the capture's side file (the picture without it under it, and as the original); the editor opens the capture with it, to move, delete or restyle |
| Quick styles in themes, favourites | Has | |
| **Themes of your own** | Has | the theme menu's "New theme from the current colours…": four colours (the ones in use first, each changeable) and a name, kept in the editor's settings and listed after the built-in four; deleted from the same menu |
| **Effect presets** | Has | at the top of the Effects pane: eight ready-made (Soft Shadow, Photo Frame, Rounded Card, Torn Paper, Old Photo, Faded Edges, Thin Line, Black and White), each one or a few effects applied with one click as one undo step; Keep as Preset… under any effect keeps its settings under a name, kept in the editor's settings (a right-click deletes one) |
| Undo and redo, also after closing | Has | |
| Flatten selected, flatten all | Has | |
| Bring forward, send back | Has | |
| **Group, align, flip annotations; select all of one kind** | Has | Edit menu and right-click: group (Ctrl+G), align, distribute, flip across or up and down, select all of the kinds selected; a group is selected, moved and turned as one |
| **Rotate annotations** | Has | a round handle over a box or a group (Shift: 15° steps), or Rotate and Flip; lines, arrows and strokes turn by their points; blur, pixelate, spotlight and magnify stay square |
| **Snap to guides** | Has | View › Rulers and Guides: rulers along the top and the left, guides dragged out of them (moved by dragging, taken away on their ruler or from their right-click menu), Guides at the Middle, Clear Guides; what is moved snaps its edges and middle to the guides and the picture's edges and middle (View › Snap to Guides); guides last while the picture is open |
| **Spell check** | Has | while a text or a callout is typed, words the English word list doesn't know (US and UK spellings both known) get a red wavy line; a right click on one offers corrections and Add to Dictionary (kept in the editor's settings); Edit › Check Spelling turns it off. The checker (symspell) and its word list (SymSpell's) are MIT, built in, so it works offline and in the browser; English only |
| **Measure a distance** | Has | the Measure tool (I): a drag reads out the length, across, down and the angle; Keep This One on the Picture (or keep each) adds a dimension line with a bar at each end and its length on a label, grouped |
| Picture grows when something is moved past its edge | Has | |
| Canvas size and colour, see-through | Has | |
| Zoom, pan, Hand tool | Has | |
| Copy, paste (a pasted picture stays an object) | Has | |
| Project file that keeps the annotations | Has | `.sgcraft` |
| Saves itself; Revert to Original | Has | |
| **Tool bar with names; tools of your choosing** | Has | names on or off (View › Tool Names); View › Customize Tool Bar…: tick the tools to show and move them up or down, kept in the settings; Usual Tools puts it back; a tool left off keeps its key |

## Effects on a picture

| Feature | Status | Notes |
|---|---|---|
| Border, drop shadow, round corners | Has | |
| Torn edge | Has | |
| **Other edges: wavy, faded, bevelled** | Has | beside the torn edge; each on the sides chosen |
| Brightness and contrast | Has | |
| **Hue, saturation, gamma** | Has | with lightness, in one effect |
| Greyscale, invert | Has | |
| **Sepia, sharpen, blur all** | Has | |
| **Colour replaced by another** | Has | Effects › Replace a Colour… and the Effects pane: a colour, chosen or picked from the picture with the eyedropper, and those within a tolerance of it, become another everywhere or only where they join the point picked; near colours keep their shade and smooth edges take the new colour; kept to a selected area too. Make a Colour Transparent (Image menu) turns one see-through |
| **Perspective, page curl, reflection** | Has | the annotations move with a tilt; a page curl at any corner |
| **Watermark (a picture or words)** | Has | in a corner, the middle or all over, turned; over the picture, or under its words and lines |
| **Capture details written on the picture (time, program)** | Part | the time (from the capture's file), what it is of and its size, in a corner or on a strip below; captures don't record the program yet, so its name is typed |
| **Effects on a selected area only** | Has | the colour changes, sharpen and blur keep to the area selected on the picture |
| Effects added at capture time | Has | by a capture preset: border, round corners, torn edge, shadow, grey, made by the editor's own effects |

## Pictures: changing, keeping, sharing

| Feature | Status | Notes |
|---|---|---|
| Resize, rotate a quarter turn, flip | Has | |
| **Rotate by any angle** | Has | Image › Rotate and Flip › Rotate by an Angle…: the picture grows to hold the turned one, the corners see-through or a colour, the annotations turned with it |
| **Templates: pictures laid out in a frame with words** | Has | Image › Template…: steps in a row, down the page, in a grid, or before and after, with a title and each step's words; every part stays an annotation |
| **Several pictures combined into one** | Has | Image › Combine Pictures…: side by side, stacked or in a grid, with recent captures or picture files |
| Many pictures changed at once (batch) | Has | Batch Changes (File menu, or All captures for the pictures shown): resize, the effects, grayscale, invert, trim, in order; a format and a name pattern; each saved beside its picture, none written over |
| Print | Part | the Image Editor's File menu and Ctrl+P, through Windows' own printing window; greyed out elsewhere |
| Recent captures strip; delete to the bin | Has | |
| Pin captures in the strip | Has | right-click a capture in the strip, Pin to the Front: the pinned stay first, with a pin on them, and are remembered |
| All captures by day, with search | Has | |
| Captures sorted by program or website, tags, favourites | Part | All captures filters by favourites, the program a window was taken from and tags, and searches them and the window's title; a website only by its page's title |
| Captures folder of your choosing | Has | |
| PNG, JPEG, WebP, BMP, GIF, PDF | Has | |
| **TIFF**, HEIF | Part | TIFF opened and saved (Export As, the command line; LZW-compressed, see-through kept), in the browser too; HEIF isn't opened: there is a pure-Rust decoder under MIT or Apache-2.0 (heifer; the other, heic, is AGPL), but HEIF pictures are HEVC, which is covered by patents, so shipping a decoder for it is the owner's call |
| Multi-page PDF of several captures | Has | All captures' PDF of These (the pictures shown, or those chosen with Ctrl+click), or Batch Changes' "One PDF, a page each": a page for each picture, changed first if asked, saved where the owner says |
| Copy, save | Has | |
| Open in another program | Part | Open With… (the File menu: the picture as shown; a capture's right-click menu in the strip: its file) in Windows' own window for choosing a program, and Show in Folder; greyed out elsewhere |
| Links, uploads, chat and cloud drives | Not planned | needs a network service |

## Editing a video: the timeline

| Feature | Status | Notes |
|---|---|---|
| A row for each track and layer; reorder, rename, hide, mute | Has | |
| **Lock a track; play one track alone** | Has | the padlock and S in its header (or its right button): a locked track's clips stay as they are (cuts, speeds and holds of the whole video still keep it in time); while any sound track plays alone only those are heard |
| Zoom the timeline, scroll it | Has | |
| Play, step a frame, full screen | Has | |
| **Faster and slower playback keys (J, K, L)** | Has | J plays backwards, K pauses, L plays on; pressed again, 2, 4 and 8 times as fast (the Play menu too). Playing on faster has its sound, voices at their own pitch; backwards as recorded has its sound turned round, faster backwards is silent |
| Split one track or every track | Has | |
| Cut leaving a gap, or closing it | Has | |
| Trim, slide a clip | Has | |
| Choose a stretch; play it | Has | |
| **Push later clips along when moving one** | Has | Shift-drag a clip: every later clip of its track (and of its group's tracks) moves with it |
| **Join pieces back together** | Has | a clip's right button joins it with the next when that goes on from where it stops (split apart, not moved); a track's right button joins all its pieces, the ruler's every track's. Revert to Recording brings every track back whole |
| **Hold a frame (freeze)** | Has | H, the Edit menu or the ruler's right button: every track waits while the picture is held, the sound silent; its end dragged, or its panel, holds it longer |
| **Speed up or slow down a stretch** | Has | a quarter as fast to eight times (any speed from a tenth to ten times by command), every track together; voices keep their pitch |
| **Play a clip backwards** | Has | a clip's right button or its panel (and the clips grouped with it): the picture and the sound turned round; captions and keys pressed aren't shown on it |
| **Line up clips by their sound** | Has | a sound clip's or a sound item's right button (or the item's panel), Line Up by Sound With: it moves so its sound is in step with the other's, their loudness and then their waves compared at every shift within 5 s, on this computer (the same voice recorded twice) |
| **Groups of clips** | Has | a clip grouped with the other tracks' clips over the same time (its right button): they move together; Ungroup takes them apart |
| Markers; chapters | Has | M marks the playhead; the markers are the MP4's chapters (both kinds of MP4) |
| **Clip colours** | Has | eight, from a clip's right button |
| **Replace a clip keeping its effects; copy effects** | Has | an item's right button or its panel (Effects): Replace With shows another file of the Media bin, of a kind for its layer, keeping its time, place, look, motion and animations. Copy Effects and Paste Effects: an item's look, entrance, exit and keyframes onto another item; a clip's colours and crop (a picture's), fades (a sound's) and the transition into it onto another clip. The recording's own clips can't be replaced (they are the recording) |
| Undo and redo; edits saved by themselves | Has | |
| Library of titles, lower thirds, callouts, arrows, blur, transitions | Has | |
| Your own pictures, sounds and videos brought in | Has | |
| Lottie animations brought in | Has | |
| **Subtitle files brought in** | Has | File › Import Subtitles: SubRip (.srt) or WebVTT (.vtt) timed on the video as it is now, made its captions (instead of those it had); each line follows its speaker's track from then on (`You:` and `Others:` lines theirs, the rest the microphone's) |
| Items moved and resized on the picture, snapped to its middle and edges | Has | |
| **Rotate an item; crop a clip** | Has | an item's panel turns it (Turn); a picture's or a video's crops its sides (Frame); a screen or camera clip's panel crops its edges, what's left filling the picture, its shape kept |
| **Pin a picture onto a slanted surface** | Has | a picture or a video on a layer: its panel's Corner Pin, then its four round handles dragged anywhere on the picture; it's drawn in perspective between them (while pinned it isn't turned, rounded or bordered) |

## Editing a video: motion and effects

| Feature | Status | Notes |
|---|---|---|
| Items come and go: fade, slide, pop | Has | |
| **Zoom into part of the screen and back (zoom and pan)** | Has (0.1.55) | smooth or at once; touching zooms pan from one to the next |
| **Zoom that follows the clicks by itself** | Has | Zoom on the Clicks (the Library's Zoom and Pan section, or the Edit menu): a zoom for each run of clicks the recording kept, in a little before the first and out a while after the last, those close together panning from one to the next; ordinary zooms, to change or delete |
| **Keyframe animation (place, size, opacity over time)** | Has | keyframes added at the playhead from the item's panel (Motion), eased from one to the next; moving or resizing it then changes the keyframe there; a zoom's keyframes pan it |
| **Motion along a path** | Has | an item with two keyframes or more: Along a curved path (its panel's Motion) takes it through their places on a smooth curve without stopping at them, easing out of the first and into the last; while it's chosen its way shows on the picture, a dot at each keyframe's place |
| Transitions as items (fades, dips, wipes, stingers) | Has | |
| **Transitions between two clips** | Has | a cross-fade, through black, wipes four ways, a push and a circle where two clips of a track meet, across the cut; sound clips cross-fade. From the clip's panel, the right button, or the Library's Between Clips dropped on the cut |
| **Words that draw themselves on** | Has | a title's, a lower third's or a callout's Entrance or Exit, Write on: its words written on letter by letter from the first, line after line, the letter being written fading on (rubbed off the same way going); anything without words wiped on from its left. The highlight, circle and arrow draw themselves on as they always did |
| **Keys pressed shown as a callout** | Has | the Keys Pressed section: shown or not, where, how large, light or dark keys |
| Progress bar, countdown | Has | |
| Camera shape and corner | Has | |
| **Camera background removed (no green screen)** | Has | the background model looks at four of the camera's pictures and learns the room; each picture keeps what differs from it (the camera kept still) |
| **A colour removed (green screen)** | Has | the camera's panel: the colour and how far from it still goes; its glow on you taken out |
| **Colour adjustments and looks** | Has | a screen or camera clip's brightness, contrast, colour and tint, in its panel; eight ready-made looks (warm, cool, vivid, faded, dramatic, black and white, sepia, night) |
| **Border, shadow, rounded corners on a clip** | Has | a picture or a video put on a layer: a border of any colour, rounded corners and a soft shadow (its panel, Frame); the recording's own screen too, made smaller on a background colour (the side panel's Screen Frame); the camera's shapes and its border |
| **Device frames, masks, blend modes** | Has | an item's panel (Mask and Blend): shown only inside a rectangle, rounded rectangle, oval, diamond, star or heart, or with the shape cut out of it, placed and sized on the item; laid on what's under it as multiply, screen, overlay, darken or lighten. A plain laptop, phone or browser window drawn in code round the screen (Screen Frame, the screen made small enough for it) or round a picture or a video on a layer (its panel, Device round it) |
| Spotlight, blur a region | Has | |
| **Motion blur, freeze a region** | Has | the side panel's Motion Blur (or a zoom's panel): items sliding, growing or moving by their keyframes, and zooms going in, out or panning, blurred along their way (five moments of the shutter blended). Edit › Freeze a Region (or the screen track's right button): a rectangle of the screen held as it was at its start while the rest plays on, an item on a layer, dragged and sized on the picture or in its panel; it stays with the screen through zooms (the pointer still shows over it) |
| Pointer: larger, smoothed, rings, hidden | Has | |
| **Pointer hidden for a stretch, a pointer of another look, its path edited** | Has | hidden for a stretch: the Pointer panel with a stretch chosen, or the screen track's right button (its clicks neither seen nor heard there; a dashed line on the screen's row; it stays with what the screen showed through later edits). The Pointer panel's Pointer: as recorded, or a white or black arrow, a hand, a dot or a ring drawn in code in its place. Edit Its Path in the Chosen Stretch: points along its way (one every half second) shown on the picture while the playhead is in it, dragged where it should go; it goes through them on a smooth curve, its clicks' rings with it; Straighten, or As Recorded again |
| Pointer highlight, spotlight, magnifier, click sounds | Has | the click sound is made in code |
| Clickable areas and quizzes in the video | Not planned | they need a web player |

## Editing a video: sound and words

| Feature | Status | Notes |
|---|---|---|
| Waves drawn; volume for each track | Has | |
| **Volume changed along a clip** | Has | a sound track's volume line: double-click a clip's wave for a point (or the track's right button), drag points up and down and along; it stays with the sound through every edit |
| **Fade in and out** | Has | a sound clip at its start and end |
| **Background noise taken out** | Has | the steady noise learnt from the track itself and turned down, on this computer (no trained model) |
| **Levels evened out; music lowered under the voice** | Has | Even Out the Levels gives each sound track the volume that makes it as loud as the others (measured on this computer); Lower the rest under the voice turns the computer's sound, music and media down while the microphone is heard |
| **Sound of a video apart from its picture; mono** | Has | a video on a layer: Separate the Sound (its panel or right button) puts its sound on a sound layer of its own at the same moment and leaves the video silent, each then moved, trimmed and deleted on its own. Mix to Mono: a sound track (its right button, its clip's panel) or a sound layer |
| **Narration recorded over the video** | Has | the side panel's Narration: Record Narration turns the microphone on (Video Production's, or another chosen from the list) and plays the video from the playhead without its sound while you speak; Pause, Stop or the video's end ends it, and the take (a WAV beside the recording) goes on a sound layer of its own, Narration, from where it began. Desktop only: greyed out in the browser |
| Captions made on this computer (You and Others) | Has | |
| Captions shown, burned in, saved as `.srt` and `.txt` | Has | |
| Transcript pane: who said what, click to go there | Has | |
| **Captions edited (a word put right, split, joined, moved)** | Has | in the transcript pane; kept with the edits, undone like any edit |
| **Captions that light up word by word** | Part | View › Light Up Each Word as It's Said: the word being said in bright blue, those said in the line's colour, those to come faint, on the picture and in the video made. The speech model times lines only, so each word's time is worked out from its line's by its length, not heard |
| **Cut words by deleting them from the transcript; filler words taken out** | Part | the transcript pane: a line's words chosen and cut (its right button), Take Out Filler Words (um, uh, er and the like); where each is said goes from every track. The speech model times lines only, so a word's time is worked out from its line's by its length, not heard |
| Computer voices, translation, dubbing | Not planned | online services |

## Video: making the file

| Feature | Status | Notes |
|---|---|---|
| MP4 that plays everywhere, a smaller AV1 MP4, an animated GIF | Has | |
| Captions in the picture or beside it | Has | |
| Show in Folder, Play | Has | |
| **Just the sound (M4A, WAV)** | Has | File › Export Sound: WAV everywhere, M4A (AAC) by Windows' own encoder, so on Windows only |
| **A frame saved as a picture** | Has | at the recording's own size, in the captures folder; it opens in the picture editor |
| **Just the chosen stretch** | Has | Export Video with a stretch chosen: its captions and chapters with it |
| **See-through video** | Has | Export Video's See-through video: a QuickTime MOV of PNG pictures (written by ScreenGrabCraft's own code; no other codec), clear wherever there's nothing: the screen's track hidden, the camera's background taken away, round titles and callouts. No sound in it (Export Sound makes it apart); not WebM |
| **A watermark** | Has | Export Video's window: words, or a picture from the Media Bin, in a corner, as see-through and as large as asked; on the preview too |
| **Several at once (batch)** | Has | File › Make Videos of Several Recordings: recordings chosen together, each made as it was edited (its layers' media read where they are), beside it under its own name, one format for all, the captions in the picture or beside it; one after another, the window saying how each is getting on |
| **Templates and themes of your own** | Has | the Library's Yours: the layers' items (or the one chosen) kept under a name with their look, in the settings folder, dropped on another recording from where it's let go (its pictures and sounds found where they were). Themes: colours for bars and cards, marks and a title's words, and a face (the bundled sans, slanted, or fixed width: not any font on the computer), six that come with it and your own; Apply to This Video, and what comes from the Library takes it |
| Uploads to video sites, an online editor, an online asset store | Not planned | needs a network service |

## The gaps, ranked

By how much someone using it every day would miss each one. Built ones move to a list of their own above it.

**Built**

1. Grab Text: the text of a picture, an area of it, or an area of the screen, to copy
   (0.1.54; Windows).
2. Zoom and pan in the video editor: zoom into part of the screen and back (0.1.55).
3. Cut Out: take a strip out of a picture and close the gap (0.1.56).
4. Speed: a stretch of video played faster or slower, voices at their own pitch (#58).
5. Sound faded in and out at a clip's ends; background noise taken out of a track (#61).
6. Captions put right in the transcript: words, split, joined, moved (#64).
7. The pointer in videos highlighted, spotlit or magnified; its clicks heard (#66).
8. Markers on the timeline, kept as chapters in the MP4 (#69).
9. A frame saved as a picture; just the chosen stretch; just the sound (WAV, M4A) (#71).
10. Hold a frame: the picture held while every track waits (#73).
11. Keys pressed shown in the video: the shortcuts kept while recording, never what's typed (#76).
12. The camera's background removed, with or without a green screen (#79).
13. Transitions between clips; keyframe animation of an item's place, size and opacity (#85).
14. Volume along a clip; the rest lowered under the voice; levels evened out (#88).
15. Groups of clips, later clips pushed along, locked and solo tracks, clip colours (#90).
16. Words cut out of the video from the transcript; filler words taken out (word times estimated) (#95).
17. J, K and L playback keys; a clip played backwards, its sound too (#97).
18. Subtitle files brought in as captions; a watermark on the video (#102).
19. Items turned and framed (cropped, bordered, rounded, shadowed); a clip's colours and crop (#106).
20. Masks and blend modes for layer items; ready-made colour looks for a clip (#109).
21. Device frames round the screen or an item; the screen made smaller on a background, rounded,
    bordered and shadowed (#111).
22. Motion blur on moving items, zooms and pans; the pointer hidden for a stretch (#112).
23. Zooms that follow the clicks; pictures put one after another (the voice recorded apart) (#113).
24. Sound while playing faster (J and L), at its own pitch; a video's sound apart from its picture;
    tracks and sound layers mixed to mono (#116).
25. Pieces of a track joined back together; a clip or an item lined up with another by its sound (#117).
26. Media replaced keeping its effects; effects copied from a clip or an item and pasted on another (#118).
27. Motion along a curved path; a picture pinned onto a slanted surface by its corners (#119).
28. Words that write themselves on; a region of the screen frozen while the rest plays on (#120).
29. A pointer of another look drawn in its place; the pointer's path redrawn through points dragged on the picture (#121).
30. The camera's background blurred or replaced by a picture; captions lit up word by word (word times estimated) (#122).
31. A see-through video (a MOV of PNG pictures); several recordings made into videos at once (#123).
32. Templates and themes of your own in the video editor (#124).
33. The sound turned round while playing backwards (J), as recorded (#125).
34. Narration recorded over the video in the editor, onto a sound layer of its own (#126).
35. A narrated video made only of pictures: a new video of them, no screen recording needed (#127).
36. Scrolling capture that scrolls by itself (Windows; #59).
37. Capture presets; areas of a set size; shapes held while dragging (#62).
38. Drawing on the screen while recording (Windows; #65).
39. Tags, favourites, and captures sorted by the program they were taken from (#68).
40. Print; batch changes; captures on a timer or at a set time (#72).
41. A picture per click made into a how-to guide (#74).
42. Several areas in one capture; a freehand area (#78).
43. A multi-page PDF of several captures (#80).
44. Captures pinned at the front of the Recent strip (#83).
45. A countdown before a recording; a still from the camera (#84).
46. A border round the camera, and the camera mirrored (#87).
47. QR codes read and made (#91).
48. Capture presets with effects added as they're taken, a folder and a format of their own, and
    `--preset <name>` (#93).
49. Open With: a capture in a program of the owner's choosing (Windows; #94).
50. The camera full screen, swapped with the screen while recording (#98).
51. Captures kept in a preset's own folder listed in the Recent strip and All captures (#99).
52. The how-to guide saved as a word-processor document and as slides (#101).
53. A still from the camera at the camera's own size (#103).
54. A recording cut short by a crash kept and said when the service starts again (#104).
55. Open menus, drop-downs and tooltips snapped to like windows (Windows; #107).
56. Step badges as letters or Roman numerals, and other badge shapes (#60).
57. Polygons, triangles and stars; curved arrows and more head shapes (#63).
58. A stamp library with search, faces, and pictures of your own as stamps (#67).
59. Any font on the computer (#70).
60. More effects: hue, saturation and gamma, sepia, sharpen, blur, wavy and faded edges, a bevel,
    perspective, a page curl, a reflection, a watermark, capture details; on a selected area (#75).
61. Rotate, group, align, distribute and flip annotations; select all of one kind (#77).
62. Magic wand, an eraser, ellipse and freehand selections; invert the selection (#81).
63. Templates; several pictures combined into one (#82).
64. Personal details hidden by themselves: emails, phone and card numbers, addresses (Windows; not faces; #86).
65. Simplify; a part of the picture lifted and moved, filled where it was (#89).
66. Edit text in the picture: a line read covered and typed over in its size and colour (Windows; #92).
67. Polygons of points clicked one by one; bent lines; callouts with several tails (#96).
68. Gradient fills; themes of your own for the quick styles (#100).
69. Effect presets; the picture rotated by any angle (#105).
70. Rulers, guides and snapping; measuring a distance (#108).
71. A tool bar of your choosing; the captured pointer as an annotation to move or delete (#110).
72. TIFF opened and saved; a colour replaced by another (#114).
73. Spell check while text and callouts are typed, with an MIT checker and word list.
74. Several microphones and cameras recorded at once, each on a track of its own.

**Still to build**

What is left either waits on a decision of the owner's or finishes something already built:

1. HEIF pictures: there is a decoder with an open licence, but HEIF pictures are HEVC video
   pictures, which are under patents.
2. HDR screens captured in their true colours.
3. Faces found by Hide Personal Details: there is no face finder with a clear licence yet.
4. The picture's own font matched when its text is edited in place.
5. A whole web page captured from its address, and pictures from a phone: both need the network.
6. Printing from any program into the editor: it needs a printer driver.
7. Parts of a window (a button, a pane) snapped to: Windows finds them only through calls made
   with `unsafe`, which this project doesn't use.
8. Drawing on the screen while recording, on macOS and Linux; desktop icons hidden while recording.
9. Captures sorted by website: a capture of a browser can't see the page's address, only its title.
10. Capture details on the picture: the program a window was taken from is in the captures'
    library now, but the details still ask for it to be typed.
11. Captions lit up word by word, and words cut from the transcript: the speech model times whole
    lines, so the times of the words are estimated.
