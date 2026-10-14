# Releasing ScreenGrabCraft

> **Last reviewed:** 2026-10-14 · **Last updated:** 2026-10-14 · **Change:** major (first version: the packaging scripts, the release workflow of ScreenGrabCraft's own, the persistent capture service on every platform) · **Status:** the scripts and workflows are written and statically checked, and the MSI, `.deb`, `.rpm` and `.tar.gz` were built from stand-in programs; **nothing has been built from the finished programs yet, and no workflow has run on GitHub**.

The canonical recipe is [craftrules `release/playbook.md`](https://github.com/storytold/craftrules/blob/main/release/playbook.md);
this file has the ScreenGrabCraft specifics. The scripts in `packaging/` are MailCraft's with
ScreenGrabCraft's names, and differ from them only where the program does: three programs instead
of two, a capture service that runs all the time, the editor's file types, and an installer
identity of its own. So a release has exactly the files a Craft app's has, under the same names.

ScreenGrabCraft lives outside the storytold organisation, so it cannot use the organisation's
shared release workflow (that one needs the organisation's runners and its `release`
environment). `.github/workflows/release.yml` and `package.yml` are ScreenGrabCraft's own and run
on GitHub's standard runners in any repository, a fork included. If the repository moves into the
organisation, call `storytold/craft-common-actions/.github/workflows/release.yml` instead. The
quick checks do use the organisation's shared workflows, which work from outside it:
`packaging-lint.yml`, `freebsd.yml` (build and test in a FreeBSD VM) and `windows-arm64.yml`
(install the ARM64 MSI on Windows 11 ARM64).

The three programs, in every package:

| Program | What | Windows subsystem |
|---|---|---|
| `screengrabcraft` | The capture service: tray icon (menu bar on macOS), Print Screen and the other capture shortcuts. Starts at sign-in and is restarted if it crashes. | GUI |
| `screengrabcraft-editor` | The annotation editor; opens `.png .jpg .jpeg .gif .bmp .webp` and its own projects (`.sgcraft`) | GUI |
| `screengrabcraft-cli` | The command-line program | console |

1. Bump the version: `cargo xtask version set 0.2.0` (the one source of truth is
   `[workspace.package] version`; the programs show it in `--version`, with the commit from
   `SCREENGRABCRAFT_BUILD_SHA`), commit, and get `cargo xtask ci` green.
2. Tag that commit `v0.2.0` and push the tag. The workflow refuses a tag that isn't the version in
   `Cargo.toml`, then builds:

| Platform | Artifacts | Signing |
|---|---|---|
| macOS | universal `ScreenGrabCraft.app` and `ScreenGrabCraft Editor.app` in one `.dmg`, universal CLI zip | Developer ID + notarization (`APPLE_*` secrets) |
| Windows | x64, x86 and arm64 `.msi` + portable `.zip` (the three programs) | Azure Trusted Signing (`AZURE_*` secrets) or a certificate (`WINDOWS_CERTIFICATE*`) |
| Linux | x86_64 and aarch64 `.AppImage` (+ `.zsync` for AppImageUpdate), `.flatpak` bundle, `.deb`, `.rpm`, `.tar.gz`; Flathub manifest | checksums |
| FreeBSD | x86_64 `.tar.gz` | checksums |
| Web | `screengrabcraft-web-<version>.zip` (the editor as a static site: `index.html`, wasm, glue) | none |

3. The run publishes ONE GitHub release, `ScreenGrabCraft v<version>` on that tag, with every file
   and a `SHA256SUMS.txt` (`sha256sum` format; `packaging/release-notes.md` is its text). Running
   the same tag again replaces the files of that release. A version with a suffix (`0.2.0-rc.1`)
   is published as a pre-release.

The release files, 23 of them with `SHA256SUMS.txt`:

```
screengrabcraft-<ver>-windows-{x64,x86,arm64}.msi
screengrabcraft-<ver>-windows-{x64,x86,arm64}-portable.zip
screengrabcraft-<ver>-macos-universal.dmg
screengrabcraft-cli-<ver>-macos-universal.zip
screengrabcraft-<ver>-linux-{x86_64,aarch64}.{AppImage,AppImage.zsync,deb,rpm,tar.gz,flatpak}
screengrabcraft-<ver>-freebsd-x86_64.tar.gz
screengrabcraft-web-<ver>.zip
SHA256SUMS.txt
```

Signing secrets (`APPLE_*`, `WINDOWS_CERTIFICATE*` or `AZURE_*`, the same names as MailCraft's)
are repository secrets; without them the builds are unsigned (ad-hoc on macOS) and the scripts say
so, so forks still build. A FreeBSD build that fails doesn't hold the release back: it then has no
FreeBSD tarball.

Dry run: `gh workflow run release.yml --ref <branch>` builds everything on any branch and keeps
the files as workflow artifacts without publishing anything (FreeBSD only when asked for: it is
slow).

## ArtCraft Launcher

The launcher installs a Craft app from its GitHub releases, finding the right file by name
(`<app>-<version>-<os>-<arch>.<ext>`) and checking it against `SHA256SUMS.txt`. What it needs of
ScreenGrabCraft, and where each part stands:

| Needs | State |
|---|---|
| One release per version with Craft file names and `SHA256SUMS.txt` | The workflow above produces it; no release has been published yet |
| An icon in the Craft style, for each program | `cargo xtask icon` (`packaging/icons.sh`) draws both into `assets/app-icon/`; `packaging/check-icons.sh` checks every size the packages use |
| A row in the launcher's catalog (`crates/core/src/catalog.rs` in `storytold/craft-launcher`): id, name, tagline, colours, `owner/repo`; for an app outside the organisation also its own reverse-DNS id (`io.github.billsecond.screengrabcraft`) and publisher (the "community apps" group) | A pull request on that repository, after the first release; the 128 px icon goes with it |

## What the installers do

- **Windows (`.msi`, WiX 5, `packaging/windows/screengrabcraft.wxs`):** per-machine install into
  Program Files\ScreenGrabCraft; Start Menu shortcuts "ScreenGrabCraft" (the capture service) and
  "ScreenGrabCraft Editor"; App Paths entries for both. The capture service starts at every
  sign-in through the `ScreenGrabCraft` value under
  `HKLM\Software\Microsoft\Windows\CurrentVersion\Run` (`"…\screengrabcraft.exe" --background`),
  removed on uninstall; each person can turn it off in the app or in Task Manager › Startup apps.
  The finish page's "Launch ScreenGrabCraft" (ticked) starts it the same way. Upgrading or
  uninstalling closes it first: WM_CLOSE to every `screengrabcraft.exe`, then whatever is left
  (the windowless supervisor) is terminated, with exit code 0 and no reboot prompt; that step is
  moved to the start of the install script so it runs before any file is removed. The editor is
  offered in "Open with" and in Settings › Default apps for `.png`, `.jpg`, `.jpeg`, `.gif`,
  `.bmp` and `.webp` without taking any of them over; `.sgcraft` projects are ScreenGrabCraft's
  own (`ScreenGrabCraft.Project`) and open in the editor. The licence page is built from
  `LICENSE-MIT`, `LICENSE-APACHE` and the fonts' OFL. The portable `.zip` holds the three programs
  and the docs and registers nothing.
- **macOS (`.dmg`):** `ScreenGrabCraft.app` (`io.github.billsecond.screengrabcraft`, a menu-bar
  app: `LSUIElement`, no Dock icon) and `ScreenGrabCraft Editor.app`
  (`io.github.billsecond.screengrabcraft.editor`, a viewer for PNG, JPEG, GIF, BMP and WebP and the
  owner of the exported `io.github.billsecond.screengrabcraft.project` type, `.sgcraft`), both
  universal (arm64 + x86_64), hardened runtime, macOS 11 or later, next to an `Applications` link.
  The capture service registers its own LaunchAgent (KeepAlive) on first run; the DMG installs
  nothing of the kind. Screen capture needs the owner's Screen Recording permission, which the app
  asks for.
- **Linux (`.deb`, `.rpm`):** `/usr/bin/screengrabcraft`, `screengrabcraft-editor` and
  `screengrabcraft-cli`; desktop entries `io.github.billsecond.screengrabcraft.desktop` and
  `io.github.billsecond.screengrabcraft.editor.desktop` (pictures and projects), the
  `application/x-screengrabcraft-project` MIME type for `.sgcraft`, AppStream metadata for both,
  the icons. The capture service starts at sign-in through
  `/etc/xdg/autostart/io.github.billsecond.screengrabcraft.desktop` (a conffile; `--background`);
  `/usr/lib/systemd/user/screengrabcraft.service` (`--tray`, `Restart=on-failure`, part of
  `graphical-session.target`) is installed as the alternative and not enabled:
  `systemctl --user enable --now screengrabcraft.service` after turning the app's own start at
  sign-in off. The binaries need glibc 2.35 or later; the tray needs a StatusNotifierItem host
  (KDE, Xfce, Cinnamon, MATE; GNOME with the AppIndicator extension); on Wayland the pictures come
  from `xdg-desktop-portal`.
- **AppImage:** starts the capture service; the editor and the CLI are inside too. No autostart
  entry or unit: the capture service registers the AppImage itself.
- **Flatpak (`io.github.billsecond.screengrabcraft`):** one app holding the three programs; the
  command is the capture service and the editor's desktop entry and icons are exported too.
  Permissions: Wayland (X11 as fallback), IPC, the GPU, Pictures (captures go to
  Pictures/ScreenGrabCraft) and Documents, and `org.kde.StatusNotifierWatcher` for the tray; no
  network. Starting at sign-in goes through the Background portal. The two manifests in
  `packaging/linux/flatpak/` (from source for Flathub, and the release bundle repackaging the
  tarball) must agree on runtime, SDK and permissions; packaging-lint checks it.
- **`.tar.gz` (Linux, FreeBSD):** `bin/` with the three programs; `share/` with the desktop
  entries, metainfo, MIME type, icons, docs, and the autostart entry under
  `share/screengrabcraft/autostart/` to copy where the desktop reads it; on Linux also
  `lib/systemd/user/screengrabcraft.service`.
- **Web (`.zip`):** the editor as a static site with relative URLs only, sample `_headers` and
  `.htaccess`, and `HOSTING.md` (`packaging/web/README.md`).
- **Product identity:** the MSI's UpgradeCode and component GUIDs are ScreenGrabCraft's own. Never
  copy them from or into a sibling Craft app (Windows would treat the two as one product).

## What the packages expect of the programs

- `--version` on all three, printing and exiting without a display, a tray host or D-Bus
  (`package.sh` and the CI checks call it; on Windows only the CLI is run).
- `screengrabcraft --background` (the sign-in entry point: a supervisor running the tray process,
  restarting it after an abnormal exit, exiting when the tray process exits normally) and
  `--tray` (the tray process alone, for launchd and systemd). The MSI ends both by WM_CLOSE and,
  failing that, TerminateProcess with exit code 0, so exit code 0 must mean "don't restart".
- `SCREENGRABCRAFT_NO_PREFS=1`: no settings read or written, no sign-in registration, no
  single-instance lock (the CI smoke tests start the programs with it).
- `SCREENGRABCRAFT_REQUIRE_WINRES=1` (set by `package.ps1`): the build fails if the Windows icon
  or VERSIONINFO can't be embedded.

## Building the packages on your own machine

Windows (needs the MSVC Rust toolchain, the Windows SDK and the .NET SDK):

```
dotnet tool install --global wix --version 5.0.2
wix extension add -g WixToolset.UI.wixext/5.0.2 WixToolset.Util.wixext/5.0.2
pwsh packaging/windows/package.ps1 -Arch x64
```

Linux (build on Ubuntu 22.04 so the result runs on newer systems; these are the packages the
release workflow installs):

```
sudo apt-get install -y build-essential pkg-config libxkbcommon-dev libwayland-dev libgtk-3-dev \
  libx11-dev libxrandr-dev libxi-dev libgl1-mesa-dev desktop-file-utils appstream zsync
# nfpm 2.47.0 from https://github.com/goreleaser/nfpm/releases, then:
packaging/linux/package.sh
```

Both write to `dist/release/`. Also: `packaging/macos/package.sh --arch universal`,
`packaging/linux/flatpak-bundle.sh` (after `package.sh`), `packaging/freebsd/package.sh` (on
FreeBSD) and `packaging/web/package.sh` (needs trunk 0.21.14). `packaging/check-icons.sh` checks
the app icons.

## What has been checked, and what has not

Checked on 2026-10-14, on a Windows 11 machine and WSL Ubuntu 22.04 and 24.04 distributions,
before the programs were finished:

- actionlint 1.7.12 (with shellcheck) on every workflow, and shellcheck 0.8.0 on every
  `packaging/**/*.sh`: clean.
- The packaging-lint checks run by hand: `xmllint` on the `.wxs`, the plists and the Linux XML;
  the `.wxs` icon ids and no GUID twice; `desktop-file-validate` on the three desktop entries
  (one hint: the capture service's entry names two main categories, Graphics and Utility, as
  intended); `appstreamcli validate --no-net --explain` on both metainfo files with appstream
  0.15.2 (Ubuntu 22.04, as the release build uses: one info, the `developer` tag it doesn't know
  yet, as MailCraft has) and 1.0.2 (Ubuntu 24.04, as the lint uses: clean); the two Flatpak
  manifests agree; the PowerShell scripts parse; `packaging/check-icons.sh` passes on the drawn
  icons. The plists parse once substituted, with the right identities, `LSUIElement`, document
  types and exported type.
- `package.ps1 -Arch x64 -SkipBuild` with stand-in programs (Windows' own GUI and console
  programs under ScreenGrabCraft's names): the PE checks, the licence RTF, the MSI and the
  portable zip were made (it stopped at the CLI smoke test, as it should with a stand-in).
  `wix build` succeeds for x64, x86 and arm64 without warnings; `wix msi validate` shows no ICE
  errors, only ICE61 (same-version upgrades, on purpose) and ICE69 (registrations naming the
  editor's file from another component, as MailCraft's do). The decompiled MSI has the identity,
  the Run value, both shortcuts, the finish-page action and the close step after
  InstallInitialize.
- `packaging/linux/package.sh --skip-build --formats "deb rpm tar"` with stand-in programs (in
  Ubuntu 22.04 with nfpm 2.47.0): the `.deb` has the depends, recommends and description above,
  the autostart entry as its only conffile, the systemd unit, both desktop entries, both metainfo
  files, the MIME type, the icons and the docs; the `.rpm` the same, the autostart entry as
  `%config`; the tarball the layout above.

Not checked yet: anything built from the real programs; the MSI installed and uninstalled for
real (it needs an administrator; the workflow does it on its runner, with the capture service
running, and checks the uninstall closed it); the finish page's launch; the x86 and arm64 Windows
builds on their machines; the AppImage, the Flatpak bundle, macOS (signing, notarization, the
DMG; and it has no DMG artwork yet: `packaging/macos/dmg/README.md`), FreeBSD and the web zip;
the Linux smoke test (the `.deb` installed, the editor and `screengrabcraft --tray` staying up
under Xvfb); signing; the release workflow on GitHub, which will need fixing on its first run.

## Revision history

| Date | Change |
|---|---|
| 2026-10-14 | First version: packaging for Windows, macOS, Linux, FreeBSD and the web from MailCraft's; the persistent capture service on each platform; the editor's file types; a release workflow of ScreenGrabCraft's own; what has and hasn't been checked |
