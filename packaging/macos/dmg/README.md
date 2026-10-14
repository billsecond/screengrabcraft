> **ScreenGrabCraft has no DMG artwork yet.** Until someone draws a background for it (original
> work, listed in ATTRIBUTION.md) and writes the two files below, `package.sh` makes a plain DMG:
> the two apps and the `Applications` link in an ordinary Finder window. The notes below describe
> the files once they exist.

# macOS DMG window

What Finder shows when the DMG opens: a background with `ScreenGrabCraft.app`,
`ScreenGrabCraft Editor.app` and the `Applications` link. `package.sh` copies these files into the
image when both exist; nothing here is generated at build time, so the DMG builds with one
`hdiutil create -srcfolder` (no Finder scripting on CI, nothing extra installed in the signing
job). `hdiutil makehybrid`, which some Craft apps use, is not used here: on current macOS it
attaches Finder information to every file, and the apps' signatures don't hold with that.

| File | What |
|---|---|
| `background.tiff` | The background at 1x and 2x in one HiDPI TIFF (sRGB, small: no noise or grain). Goes to `.background/background.tiff`. |
| `dmg-layout.DS_Store` | Finder's view settings for the volume: window size, icon size, the positions of the two apps and `Applications`, and the background. Goes to `.DS_Store` in the image; named so it isn't mistaken for (or ignored like) a Finder-generated `.DS_Store`. |

MailCraft's `packaging/macos/dmg/generate.py` (from VectorCraft's) writes both from an SVG and a
layout; adapt it for three icons when the artwork exists.

## The volume name has no version

The mounted volume is called `ScreenGrabCraft`, not `ScreenGrabCraft <version>`. The layout points
at the background through an alias that includes the volume name, and Finder resolves it by that
name: with a versioned name the window keeps its size and icon positions but shows no background.
The DMG file name (`screengrabcraft-<version>-macos-<arch>.dmg`) still carries the version.

## Rules

- **Finder draws the icon labels in black in light and dark mode** when a window has a background,
  so the area under the icons stays light.
- **Nothing goes inside the icon boxes:** artwork keeps 10 pt clear of each icon box and of the
  label strip under it.
