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
| Open menus and tooltips caught as they were | Part | the screen is frozen when the key is pressed, so open menus are in it; menus aren't found and cut out by themselves |
| Parts of a window (a button, a pane) snapped to | Missing | |
| Freehand-shaped area | Missing | |
| Several areas in one capture | Missing | |
| Fixed size or fixed place area | Part | the Recorder has fixed sizes for videos; still pictures don't |
| Aspect ratio held while dragging (square, 16:9, 4:3) | Missing | |
| The program's own windows kept out of the picture | Has | the editor steps aside |
| The mouse pointer in the picture | Part | drawn into the picture when Settings say so; not a separate object to move or delete later |
| **Grab Text: the text in an area straight to the clipboard** | Has (0.1.54) | Windows' own text reader, on this computer; Windows only for now |
| Scrolling capture, scrolled by hand | Has | |
| Scrolling capture that scrolls by itself | Missing | |
| Captures again and again on a timer (time-lapse), or at a set time | Missing | |
| A picture for each click, made into a numbered how-to guide | Missing | |
| The guide saved as a document, slides, PDF, web page or Markdown | Missing | |
| A still from the camera | Missing | |
| A whole web page from its address | Missing | |
| Printing from any program into the editor | Missing | |
| HDR screens captured in their true colours | Missing | |
| QR codes made or read | Missing | |
| Pictures sent from a phone | Not planned | needs a network service |
| Capture presets (an area type, effects and where it goes, under one shortcut) | Missing | one default for every capture (Settings, After a capture) |
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
| Camera in a corner: square, rounded, circle | Has | |
| Camera full screen, swapped with the screen while recording | Part | with the screen off the camera is the video |
| Camera border, mirrored camera | Missing | |
| Camera background blurred or replaced | Missing | |
| Each source on a track of its own, to edit | Has | |
| Pause, resume, stop by keys (F9, F10); restart; discard | Has | |
| Recording controls kept out of the video | Has | |
| Countdown before recording | Part | the capture delay; no countdown of its own |
| Longest recording | Has | Settings |
| Microphone heard as silent (muted or not allowed) said plainly | Has | after the recording |
| Background noise taken out | Missing | |
| Pointer: where it is, clicks, which pointer | Has | |
| Pointer made larger, smoothed, click rings, hidden | Has | |
| Pointer highlighted, spotlit or magnified; click sounds | Missing | |
| Keys pressed shown in the video | Missing | |
| Drawing on the screen while recording | Missing | |
| A recording survives a crash | Part | the recording file is written as it goes |
| Desktop icons hidden while recording | Missing | |
| A narrated video made from pictures | Missing | |

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
| The captured pointer moved or deleted | Missing | |
| Quick styles in themes, favourites | Has | |
| **Themes of your own** | Has | the theme menu's "New theme from the current colours…": four colours (the ones in use first, each changeable) and a name, kept in the editor's settings and listed after the built-in four; deleted from the same menu |
| Effect presets | Missing | |
| Undo and redo, also after closing | Has | |
| Flatten selected, flatten all | Has | |
| Bring forward, send back | Has | |
| **Group, align, flip annotations; select all of one kind** | Has | Edit menu and right-click: group (Ctrl+G), align, distribute, flip across or up and down, select all of the kinds selected; a group is selected, moved and turned as one |
| **Rotate annotations** | Has | a round handle over a box or a group (Shift: 15° steps), or Rotate and Flip; lines, arrows and strokes turn by their points; blur, pixelate, spotlight and magnify stay square |
| Snap to guides | Missing | |
| Spell check | Missing | |
| Measure a distance | Missing | |
| Picture grows when something is moved past its edge | Has | |
| Canvas size and colour, see-through | Has | |
| Zoom, pan, Hand tool | Has | |
| Copy, paste (a pasted picture stays an object) | Has | |
| Project file that keeps the annotations | Has | `.sgcraft` |
| Saves itself; Revert to Original | Has | |
| Tool bar with names; tools of your choosing | Part | names on or off; the tools can't be chosen |

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
| Colour replaced by another | Part | by see-through only |
| **Perspective, page curl, reflection** | Has | the annotations move with a tilt; a page curl at any corner |
| **Watermark (a picture or words)** | Has | in a corner, the middle or all over, turned; over the picture, or under its words and lines |
| **Capture details written on the picture (time, program)** | Part | the time (from the capture's file), what it is of and its size, in a corner or on a strip below; captures don't record the program yet, so its name is typed |
| **Effects on a selected area only** | Has | the colour changes, sharpen and blur keep to the area selected on the picture |
| Effects added at capture time | Missing | |

## Pictures: changing, keeping, sharing

| Feature | Status | Notes |
|---|---|---|
| Resize, rotate a quarter turn, flip | Has | |
| Rotate by any angle | Missing | |
| **Templates: pictures laid out in a frame with words** | Has | Image › Template…: steps in a row, down the page, in a grid, or before and after, with a title and each step's words; every part stays an annotation |
| **Several pictures combined into one** | Has | Image › Combine Pictures…: side by side, stacked or in a grid, with recent captures or picture files |
| Many pictures changed at once (batch) | Missing | |
| Print | Missing | |
| Recent captures strip; delete to the bin | Has | |
| Pin captures in the strip | Missing | |
| All captures by day, with search | Has | |
| Captures sorted by program or website, tags, favourites | Missing | |
| Captures folder of your choosing | Has | |
| PNG, JPEG, WebP, BMP, GIF, PDF | Has | |
| TIFF, HEIF | Missing | |
| Multi-page PDF of several captures | Missing | |
| Copy, save | Has | |
| Open in another program | Part | Show in Folder |
| Links, uploads, chat and cloud drives | Not planned | needs a network service |

## Editing a video: the timeline

| Feature | Status | Notes |
|---|---|---|
| A row for each track and layer; reorder, rename, hide, mute | Has | |
| Lock a track; play one track alone | Missing | |
| Zoom the timeline, scroll it | Has | |
| Play, step a frame, full screen | Has | |
| Faster and slower playback keys (J, K, L) | Missing | |
| Split one track or every track | Has | |
| Cut leaving a gap, or closing it | Has | |
| Trim, slide a clip | Has | |
| Choose a stretch; play it | Has | |
| Push later clips along when moving one | Missing | |
| Join pieces back together | Part | Revert to Recording brings every track back |
| Hold a frame (freeze) | Missing | |
| **Speed up or slow down a stretch** | Missing | |
| Play a clip backwards | Missing | |
| Line up clips by their sound | Missing | |
| Groups of clips | Missing | |
| Markers; chapters | Missing | |
| Clip colours | Missing | |
| Replace a clip keeping its effects; copy effects | Missing | |
| Undo and redo; edits saved by themselves | Has | |
| Library of titles, lower thirds, callouts, arrows, blur, transitions | Has | |
| Your own pictures, sounds and videos brought in | Has | |
| Lottie animations brought in | Has | |
| Subtitle files brought in | Missing | |
| Items moved and resized on the picture, snapped to its middle and edges | Has | |
| Rotate an item; crop a clip | Missing | |
| Pin a picture onto a slanted surface | Missing | |

## Editing a video: motion and effects

| Feature | Status | Notes |
|---|---|---|
| Items come and go: fade, slide, pop | Has | |
| **Zoom into part of the screen and back (zoom and pan)** | Has (0.1.55) | smooth or at once; touching zooms pan from one to the next |
| Zoom that follows the clicks by itself | Missing | |
| Keyframe animation (place, size, opacity over time) | Missing | |
| Motion along a path | Missing | |
| Transitions as items (fades, dips, wipes, stingers) | Has | |
| Transitions between two clips | Missing | |
| Words that draw themselves on | Part | the highlight and circle do |
| Keys pressed shown as a callout | Missing | |
| Progress bar, countdown | Has | |
| Camera shape and corner | Has | |
| Camera background removed (no green screen) | Missing | |
| A colour removed (green screen) | Missing | |
| Colour adjustments and looks | Missing | |
| Border, shadow, rounded corners on a clip | Part | the camera's shapes |
| Device frames, masks, blend modes | Missing | |
| Spotlight, blur a region | Has | |
| Motion blur, freeze a region | Missing | |
| Pointer: larger, smoothed, rings, hidden | Has | |
| Pointer hidden for a stretch, a pointer of another look, its path edited | Missing | |
| Pointer highlight, spotlight, magnifier, click sounds | Missing | |
| Clickable areas and quizzes in the video | Not planned | they need a web player |

## Editing a video: sound and words

| Feature | Status | Notes |
|---|---|---|
| Waves drawn; volume for each track | Has | |
| Volume changed along a clip | Missing | |
| **Fade in and out** | Missing | |
| **Background noise taken out** | Missing | |
| Levels evened out; music lowered under the voice | Missing | |
| Sound of a video apart from its picture; mono | Missing | |
| Narration recorded over the video | Missing | |
| Captions made on this computer (You and Others) | Has | |
| Captions shown, burned in, saved as `.srt` and `.txt` | Has | |
| Transcript pane: who said what, click to go there | Has | |
| **Captions edited (a word put right, split, joined, moved)** | Missing | |
| Captions that light up word by word | Missing | |
| Cut words by deleting them from the transcript; filler words taken out | Missing | |
| Computer voices, translation, dubbing | Not planned | online services |

## Video: making the file

| Feature | Status | Notes |
|---|---|---|
| MP4 that plays everywhere, a smaller AV1 MP4, an animated GIF | Has | |
| Captions in the picture or beside it | Has | |
| Show in Folder, Play | Has | |
| Just the sound (M4A, WAV) | Missing | |
| A frame saved as a picture | Missing | |
| Just the chosen stretch | Missing | |
| See-through video (WebM) | Missing | |
| A watermark | Missing | |
| Several at once (batch) | Missing | |
| Templates and themes of your own | Missing | |
| Uploads to video sites, an online editor, an online asset store | Not planned | needs a network service |

## The gaps, ranked

By how much someone using it every day would miss each one. Built ones move to a list of their own above it.

**Built**

1. Grab Text: the text of a picture, an area of it, or an area of the screen, to copy
   (0.1.54; Windows).
2. Zoom and pan in the video editor: zoom into part of the screen and back (0.1.55).
3. Cut Out: take a strip out of a picture and close the gap (0.1.56).
4. Step badges as letters or Roman numerals, and other badge shapes.
5. Polygons, triangles and stars; curved arrows and more head shapes.
6. A stamp library with search, faces, and pictures of your own as stamps.
7. Any font on the computer.
8. More effects: hue, saturation and gamma, sepia, sharpen, blur, wavy and faded edges, a bevel,
   perspective, a page curl, a reflection, a watermark, capture details; on a selected area.
9. Rotate, group, align, distribute and flip annotations; select all of one kind.
10. Magic wand, an eraser, ellipse and freehand selections; invert the selection.
11. Templates; several pictures combined into one.
12. Personal details hidden by themselves: emails, phone and card numbers, addresses (Windows; not faces).
13. Simplify; a part of the picture lifted and moved, filled where it was.
14. Edit text in the picture: a line read covered and typed over in its size and colour (Windows).
15. Polygons of points clicked one by one; bent lines; callouts with several tails.
16. Gradient fills; themes of your own for the quick styles.

**Still to build, in this order**

1. Speed up or slow down a stretch of video.
2. Fade sound in and out; take background noise out.
3. Edit captions: put a word right, split, join, move.
4. Scrolling capture that scrolls by itself.
5. Pointer highlight, spotlight and magnifier; click sounds.
6. Markers on the timeline, kept as chapters in the MP4.
7. A frame saved as a picture; just the chosen stretch; just the sound.
8. Hold a frame.
9. Keys pressed shown in the video.
10. Capture presets; fixed-size picture areas; aspect ratios held while dragging.
11. Camera background removed, or a colour removed.
12. Transitions between clips; keyframe animation.
13. Drawing on the screen while recording.
14. Volume along a clip; music lowered under the voice; levels evened out.
15. Tags, favourites, and captures sorted by program.
16. A picture per click made into a how-to guide.
17. Groups, pushing later clips along, locked and solo tracks, clip colours.
18. Print; batch changes; captures on a timer or at a set time.
19. Cutting words from the transcript; filler words taken out.
20. The rest: device frames, pinning onto surfaces, motion paths, blend modes, playing
    backwards, lining up by sound, see-through video, HDR, QR codes, measuring, spell check.
