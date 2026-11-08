# Distribution

`.github/workflows/build-executables.yml` builds the editor and the command-line tools in release
mode on each OS and uploads one **workflow artifact** per OS (private to the repository, kept 30
days). It runs after the tests on every push to `main` (called from `rust.yml`), and on pull
requests that change `packaging/`, `assets/icons/` or the workflow. Releases are separate:
`release.yml` runs only for a tag the owner pushes (`vX.Y.Z` for a stable version, or
`vX.Y.Z-alpha.N`, `-beta.N`, `-rc.N` for a pre-release, marked as such) and creates a draft
GitHub release with the same packages and a `SHA256SUMS` file (sha256sum format, one line per
asset) that the editor's self-updater checks downloads against; `release-smoke.yml` then installs
the published Windows and Linux packages and runs them once. The build receives the label as
`OPENSTITCH_RELEASE_TAG`, which the editor embeds so a release build knows its tag (see
[research/updater.md](research/updater.md)). The notes for each release are in `docs/releases/`.

| OS | Artifact | Built by | File associations |
|---|---|---|---|
| macOS (universal: arm64 + x86_64) | `OpenStitch-<label>-macos-universal.zip` holding `OpenStitch.app` | `packaging/macos/bundle.sh` (Info.plist, `.icns`, ad-hoc signature) | `.osd` (owner, exported UTI `io.github.cristianonescu.openstitch.design`), `.pes` (alternate editor) |
| Windows (x64) | `OpenStitch-<label>-windows-x86_64.zip` (portable) and `.msi` | WiX Toolset 3 from `packaging/windows/openstitch.wxs` | `.osd` default (ProgId `OpenStitch.Design`); `.pes` in "Open with" only (`OpenWithProgids`), so an existing PES handler stays the default |
| Linux (x86_64) | `OpenStitch-<label>-linux-x86_64.tar.gz` with `install.sh` | `packaging/linux/package.sh` | `.desktop` entry plus shared-mime-info types `application/x-openstitch-design` (`*.osd`) and `application/x-pes-embroidery` (`*.pes`, magic `#PES`) |
| Linux (x86_64) | `OpenStitch-<label>-linux-x86_64.AppImage` (single file, no install) | `packaging/linux/appimage.sh` (AppDir + `AppRun`, appimagetool) | Same desktop entry, MIME types, icons and thumbnailer, added per user by `--install-integration` |
| Linux (x86_64) | `OpenStitch-x86_64.flatpak` bundle (workflow `Flatpak`, separate artifact) | `packaging/flatpak/io.github.cristianonescu.OpenStitch.yml` (flatpak-builder) | Exported desktop entry and MIME types under the app id |

The Linux job validates the desktop entry (`desktop-file-validate`) and MIME XML, installs into a
temporary prefix and checks the MIME database picked up both globs; it then builds the AppImage
and tests its thumbnailer and desktop integration (see [AppImage](#appimage)). The macOS job checks the
signature, the document types and the icon in the bundle. Every job checks that its packages
carry the app icon (see [Icons and shortcuts](#icons-and-shortcuts)).

Every artifact also holds `openstitch-cli-<label>-<os>` (`.tar.gz`, or `.zip` on Windows)
with `stitchtool`, `openstitch-mcp`, the README and `examples/`. `<label>` is `main-<sha>` for
builds from `main` and the tag (e.g. `v0.2.0`) for releases; the MSI's internal version is the
workspace version. Before packaging, each job smoke-tests the binaries (`stitchtool exec
examples/badge.json`, `openstitch-mcp --help`).

## AppImage

`packaging/linux/appimage.sh <openstitch> <version> <out-dir>` lays out an AppDir (editor and
thumbnailer under `usr/bin`, the desktop entry, MIME XML, thumbnailer entry and hicolor icons
under `usr/share`, `AppRun` and the 256 px icon at the top) and packs it with
[appimagetool](https://github.com/AppImage/appimagetool) 1.9.1 and the static
[type2 runtime](https://github.com/AppImage/type2-runtime) `20251108`. Both are downloaded from
those versioned releases (never `continuous`) and checked against SHA-256 sums pinned in the
script (x86_64 and aarch64); set `APPIMAGETOOL` / `APPIMAGE_RUNTIME` to use local copies. The
type2 runtime is statically linked, so the AppImage needs no `libfuse2` on the host (it uses
FUSE 3 when present; `--appimage-extract-and-run` works without FUSE).

`AppRun` takes three extra options besides a file to open:

| Option | Effect |
|---|---|
| `--thumbnailer -s SIZE IN OUT.png` | Runs the bundled `openstitch-thumbnailer` |
| `--install-integration` | For the current user (`$XDG_DATA_HOME`, default `~/.local/share`): desktop entry whose `Exec` is this AppImage's path, `.osd`/`.pes` MIME types, icons, the thumbnailer entry, and a copy of the thumbnailer in `openstitch/appimage/` (file managers run thumbnailers outside the AppImage mount, often sandboxed); sets OpenStitch as the default for both types |
| `--remove-integration` | Removes all of the above |

Run `--install-integration` again after moving or updating the AppImage. Tools such as
AppImageLauncher or appimaged also pick up the embedded desktop entry, but not the thumbnailer.

The CI step prints `ldd` of the editor, renders a thumbnail through `AppRun`, installs the
integration into a temporary `XDG_DATA_HOME`, validates the written desktop entry, checks its
`Exec`, the MIME glob, the icon and the copied thumbnailer, then removes it again. The binaries
are built on `ubuntu-latest`, so the AppImage needs a glibc at least as new as that runner's
(Ubuntu 24.04: glibc 2.39). X11, Wayland, OpenGL and Vulkan libraries are loaded at run time
from the host, as with the tarball.

## Flatpak

`packaging/flatpak/io.github.cristianonescu.OpenStitch.yml` builds the editor against the
freedesktop 25.08 runtime with the `rust-stable` SDK extension. It installs the binary, the
desktop entry, MIME types and icons renamed to the app id, and AppStream metadata
(`io.github.cristianonescu.OpenStitch.metainfo.xml`). Sandbox: Wayland with X11 fallback, GPU,
the home folder and removable media (`/media`, `/run/media`); file dialogs go through the
desktop portal (rfd's `xdg-portal` backend).

```bash
flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak-builder --user --install-deps-from=flathub --force-clean --install build-flatpak packaging/flatpak/io.github.cristianonescu.OpenStitch.yml
flatpak run io.github.cristianonescu.OpenStitch
```

`.github/workflows/flatpak.yml` runs this build (on pull requests and pushes to `main` that
touch `packaging/flatpak/`, `packaging/linux/`, the icons or the workflow, and on demand),
validates the AppStream file with `appstreamcli`, checks the exported desktop entry, MIME types
and icons, and uploads `OpenStitch-x86_64.flatpak` (install with `flatpak install --user
OpenStitch-x86_64.flatpak`; the runtime comes from Flathub). It is a separate workflow so that
the main build does not wait for the SDK download.

Limits:
- flatpak-builder runs `appstreamcli compose` on the host, which needs the host's SVG image
  loader (Debian/Ubuntu: `librsvg2-common`) and `eu-strip` (`elfutils`); without them the build
  ends with `file-read-error` on the scalable icon or a missing `eu-strip`.
- Cargo downloads crates during the build (`build-args: --share=network`), which local
  `flatpak-builder` allows. Flathub builds offline: a submission needs the crates as sources,
  generated from `Cargo.lock` by `flatpak-cargo-generator.py` (flatpak-builder-tools) into
  `cargo-sources.json`, plus `CARGO_NET_OFFLINE=true`.
- Flatpak does not export thumbnailers, so file managers show no stitch previews for the
  Flatpak (use the tarball or the AppImage integration for that).
- The sandbox has no network, so `openstitch --listen` cannot be reached from `stitchtool` on
  the host; allow it with `flatpak override --user --share=network io.github.cristianonescu.OpenStitch`.

## Icons and shortcuts

The OpenStitch logo (a needle and its thread on a navy tile), generated with Higgsfield; see
`assets/brand/README.md`. `assets/icons/openstitch.svg` is the vector master and the Linux
scalable icon. `tools/gen_icons.py` (Pillow, plus resvg or Inkscape to rasterise) writes from it
the PNGs 16–1024, `openstitch.ico` (16/24/32/48/64/128/256), `openstitch.icns` and
`openstitch-64.rgba`, which the app embeds as its window icon; at 32 px and below the needle and
thread are drawn bolder so they stay visible. Re-run the script after changing the SVG and
commit every file it writes; the packages below take them from `assets/icons/`.

What each package carries, and what the workflow checks before it uploads the package:

- **Windows exe (zip and MSI).** `openstitch.exe` embeds `openstitch.ico` as its icon resource
  (`crates/openstitch-app/build.rs` writes a `.res` and links it with the MSVC toolchain), so
  Explorer, the taskbar and Alt+Tab show the logo for the exe itself, and the self-updater's
  in-place swap of the exe keeps it. The zip also holds `openstitch.ico`.
- **Windows MSI.** The Icon table holds `openstitch.ico`; the advertised Start-menu shortcut,
  the `OpenStitch.Design` ProgId (`.osd` files) and `ARPPRODUCTICON` (Programs and Features)
  refer to it, and `.pes` files get the installed `openstitch.ico` as their `DefaultIcon`.
  Windows Installer copies the Icon-table entry to `%WINDIR%\Installer\{ProductCode}\` under
  its table name and the shell loads it by that name's extension, so the name must end in
  `.ico` or `.exe`: v0.1.0-alpha.4 named it `OpenStitchIcon` and the Start menu, the taskbar
  (for a window started from the shortcut) and Programs and Features all showed the generic
  icon. The workflow fails if any icon name in the MSI lacks the extension, or if the exe has
  no icon resource.
- **macOS.** `OpenStitch.app/Contents/Resources/openstitch.icns` (16–1024 px, `ic07`–`ic14`),
  named by `CFBundleIconFile` in `Info.plist`; the `.osd` document type and its UTI use the same
  file. `ditto` zips the bundle with its structure. The workflow checks the plist key, that the
  file is an icon and that the zip lists it.
- **Linux tarball.** `share/icons/hicolor/<size>x<size>/apps/openstitch.png` for 16–512 and
  `scalable/apps/openstitch.svg`, with `share/applications/openstitch.desktop` (`Icon=openstitch`);
  `install.sh` copies them under the prefix and refreshes the icon cache. The workflow installs
  into a temporary prefix and checks that every size landed.
- **AppImage.** `openstitch.desktop`, `openstitch.png` (256 px) and `.DirIcon` at the AppDir
  root, which file managers and AppImage launchers read, plus the same hicolor set under `usr/
  share`; `--install-integration` installs the hicolor icons for the user. The workflow extracts
  the image (`--appimage-extract`) and checks the root files and the hicolor set.
- **Flatpak.** The hicolor icons renamed to the app id (see [Flatpak](#flatpak)).

The editor shows the logo as a splash for about a second on a plain start (no options, or only
a file to open). Click or press a key to skip it. It never shows with `--screenshot`,
`--script`, `--listen` or any option that sets up a view; `--no-splash` and Options ▸ "Show
splash at startup" turn it off.

## Opening files

The editor opens a file given on the command line (`openstitch design.osd`, or any machine
file, which is imported into a new design). Windows and Linux (tarball, AppImage integration,
Flatpak) pass the double-clicked file that way. macOS Finder does not; see *macOS double-click*
under Known limits.

## Updates

The editor updates itself (design and decision: [research/updater.md](research/updater.md);
code: `crates/openstitch-update`, `crates/openstitch-app/src/update_ui.rs`).

- **When.** A plain interactive start (at most a file to open; `--no-splash` is fine) of a
  release build starts a check on a background thread; `--screenshot`, `--script`, `--listen`
  and the other options that set up a view do not, nor does a development build (debug, or a
  binary under a cargo `target/` directory), a Flatpak, or a start with
  `OPENSTITCH_NO_UPDATE_CHECK` set. Options ▸ *Check for Updates Now* and Help ▸ *Check for
  Updates* ask on demand and also report "You have the latest version" or the error.
- **What is newer.** The build's tag (`OPENSTITCH_RELEASE_TAG` at build time; About shows it)
  is compared with GitHub's release list using semantic versioning with pre-release precedence
  (`0.1.0-alpha.3 < 0.1.0-alpha.4 < 0.1.0-beta.1 < 0.1.0`). A pre-release build sees newer
  pre-releases and stable releases; a stable build sees stable releases only unless Options ▸
  *Include pre-releases* is on. Drafts are never offered. The check is one anonymous request
  (GitHub allows 60 an hour per address); offline or on any error the start is silent.
- **The dialog.** Shows the notes of every release newer than the running one (the release
  body: `docs/releases/<tag>.md` plus GitHub's generated list), newest first. *Dismiss* closes
  it; with *Don't remind me about this version* ticked the version is remembered in `ui.json`
  (`updates.skipVersion`) and offered again only when something newer appears. *Update* is
  disabled while the design has unsaved changes.
- **Update.** Downloads `SHA256SUMS`, then the package for this install to a temporary folder,
  refusing it unless its SHA-256 matches, over HTTPS only (rustls) and only from the
  repository's release URLs. Then, per install:

  | Install | Package | How it is applied |
  |---|---|---|
  | Windows portable (exe outside Program Files) | `OpenStitch-<tag>-windows-x86_64.zip` | `openstitch.exe` is unpacked beside the running one as `.new`, the running exe is renamed `.old`, the new one takes its place, and the editor restarts; `.old` is removed at the next start |
  | Windows MSI (`%ProgramFiles%\OpenStitch`) | `OpenStitch-<tag>-windows-x86_64.msi` | The editor exits and a detached `cmd` runs `msiexec /i … /passive /norestart` (Windows asks for permission, the usual publisher-unknown prompt for an unsigned package), then starts the installed editor |
  | macOS `OpenStitch.app` | `OpenStitch-<tag>-macos-universal.zip` | The zip is unpacked beside the bundle, the bundle is renamed `OpenStitch.app.old`, the new one takes its place and is started with `open -n`; the download sets no quarantine attribute, so Gatekeeper does not ask again |
  | Linux AppImage | `OpenStitch-<tag>-linux-x86_64.AppImage` | The new image replaces `$APPIMAGE` (desktop integration keeps pointing at the same path) and is started |
  | Linux tarball | `OpenStitch-<tag>-linux-x86_64.tar.gz` | `bin/openstitch` is replaced; the thumbnailer is not (reinstall the tarball for that) |
  | Flatpak | — | `flatpak update`; the editor never checks |

  A location the user cannot write (an app bundle installed by an administrator, a system
  prefix) makes the update fail with the reason and a link to the release page. The restarted
  editor gets the same command-line arguments and shows "Updated from X to Y" in the status
  bar.
- **Settings** in `ui.json` under `updates`: `check` (default true), `includePrereleases`
  (default false), `skipVersion`.
- **Testing a build without a release:** with `OPENSTITCH_UPDATE_API` and
  `OPENSTITCH_UPDATE_DOWNLOADS` set, the client uses that API base and download prefix (plain
  http allowed) instead of GitHub. The release pipeline publishes `SHA256SUMS` since
  v0.1.0-alpha.4, the first version that checks; a release without it is never applied.

## Web editor

The design editor also runs in a web browser (#556), with nothing to install: `crates/openstitch-web`
built for `wasm32-unknown-unknown`, drawn by egui on a WebGL 2 canvas, hosted by the static page
`web/index.html`. Since #693 phase B part 2 (2026-11-07) the page runs the desktop editor itself
(`openstitch_app::EditorApp`: ribbon, panes, tools, dialogs). File ▸ Open (Ctrl+O) and Import go
through the browser's file picker; Save, Save As and every export are handed to the browser as
downloads. Nothing leaves the browser.

It runs the same engine commands as the desktop editor (`project.open`, `stitches.import
{keepSource}`, `design.export`, `project.save`). In the browser those commands read and write
in-memory files (`openstitch_engine::fsio`; on the desktop the same functions are `std::fs`, so
native behaviour is unchanged): a picked file is stored, opened and removed, and an exported file
is written, taken and handed to the browser as a download. The JEF header's time stamp comes from
the page's clock. The exports are byte-identical to the desktop build: CI job `web` opens
reference PES and DST files through the browser build's API in Node (`web/smoke.mjs`) and
compares PES, DST, EXP, VP3, XXX, CSV and G-code with `cargo run -p openstitch-web --example
web_export`, the same steps run natively; `openstitch-web`'s own tests compare those with the
desktop editor's File ▸ Open and File ▸ Export.

Desktop-only for now: the Assistant (its key lives in the system keychain), Quick Look, sending to
a machine or USB stick, printing, updates, the automation socket, the Design Database's folders,
the system clipboard (copy and paste stay inside the editor), and choosing a folder (the Design
Database, Ink/Stitch font folders, Save Template To: a page cannot browse folders). Every file
picker works (#693): open and import, background and digitizing pictures, thread charts and
lists, fonts, font pictures and name-drop lists, and since #797 also Open with "Open designs in
tabs", the Programmable Stitch Creator's and Stitch Design Factory's Open and Open Template, and
Import Shortcuts. A design, embroidery file or picture dropped on the page opens or becomes the
background. The editor's own reads and saves of user files (the background picture on the canvas,
the image wizards' preview, templates, patterns, shortcuts) go through
`openstitch_app::platform::{read_limited, read_text, write_file}`: the file system on the
desktop, the page's memory in the browser, where a save in the save folder becomes a download. The editor's preferences (the desktop's `ui.json`:
start cards, theme, units, panes, shortcuts…) are kept in the origin's `localStorage` under
`openstitch.ui` (#797): the page registers it through `openstitch_app::platform::set_prefs_store`
and every change is written back. Where the browser refuses storage (a private window, cleared or
blocked site data) the defaults apply and nothing is kept. The user settings (thread charts, My
Threads, the default chart, fabrics) are not kept between visits yet. The `.wasm` is about 17 MB
(#696).

**Browsers:** `web/e2e/page.mjs` (Playwright, version pinned in `web/e2e/package-lock.json`;
it also turns the start cards off and on with a reload after each, #797)
loads the page in Chromium, Firefox and WebKit (Safari's engine): the editor must start, File ▸
Open (Ctrl+O) goes through the browser's real file picker, and PES and DST exports written to the
editor's save folder arrive as real downloads that must be byte-identical to the native build.
Before part 2 all three passed on macOS (2026-11-07, `node web/e2e/page.mjs WEB_DIR NATIVE_DIR
INPUT`). CI job `web` runs it on
Linux: Chromium always counts; Firefox counts when Mesa (for WebGL on the GPU-less runner)
installs within 8 minutes, else it is a CI warning; WebKit needs GTK 4 and GStreamer, which the
runner lacks, so there it is a warning too. The screenshots are kept as the
`openstitch-web-screenshots` artifact. Safari itself (macOS, iPadOS) is not run; WebKit is its
engine.

**Toward the full editor (#693):** the desktop editor's library (`openstitch_app::editor`: the
viewport, hit-testing, tools and render models, without a window) builds for `wasm32` too, checked
by CI job `web`. Its desktop-only dependencies (keychain, clipboard images, the automation socket,
the updater, the Assistant's HTTPS client, the system trash) are target-gated in
`crates/openstitch-app/Cargo.toml`, and `crates/openstitch-app/src/platform.rs` holds what differs:
on the desktop each function is a pass-through to the call the editor made before (the system
clipboard, the release-tag check); in the browser the clipboard stays inside the editor.

Phase B, part 1 (2026-11-07): the editor window builds for `wasm32` as well. Its modules are
listed once in `crates/openstitch-app/src/bin_modules.rs`: the desktop binary (`main.rs`) includes
the list, and on `wasm32` the library includes it too (`extern crate self as openstitch_app`),
so `openstitch_app::EditorApp` exists for the web crate without moving any file. What a browser
cannot do is behind small stand-ins, each identical to the desktop code on the desktop:
`files.rs` (file dialogs: `rfd::FileDialog` on the desktop; in the browser the pickers return
nothing yet), `web/assistant_panel.rs` (no Assistant panel: it needs a model connection, the
keychain and a local socket), `web/update_ui.rs` (no updater), `remote.rs` (no automation
socket), the Assistant's notes compiled from `openstitch-assistant/src/notes.rs` without its
network client, and no system trash. CI job `web` builds the whole package for `wasm32`.

Phase B, part 2 (2026-11-07): `crates/openstitch-web/src/web.rs` starts `EditorApp` on the canvas
(glow renderer, no splash) inside a small wrapper frame. What differs stays in
`openstitch-app`'s stand-ins, identical on the desktop: `platform::run_job` (a thread on the
desktop; a browser page has one thread, so the background open, a picker tile's preview and the
print hand-off run at once and their result waits in the channel for the next frame),
`platform::Instant` (`web_time` in the browser, where std's clock panics), `platform::read_small`
(the in-memory files), and `files.rs`: File ▸ Open and Import ask the page for its picker
(`request_pick`); the page puts the picked bytes under `/web/open/` in memory and hands the path
back (`EditorApp::web_picked`). The Save dialog answers `/web/save/<name>` at once; after every
frame the page downloads each file written there and drops it from memory. The other pickers go
through `files::pick_then(dialog, |app, path| …)`: on the desktop the dialog runs as before and the
action runs at the start of the next frame; in the browser the page shows its picker (with every
filter, one file or several) and the action runs when the bytes are in memory. The engine reads
those files through `fsio` (`fsio::metadata` and `fsio::read`, `std::fs` on the desktop).
Dropped files arrive as egui's `dropped_files`, read with `bytes_async`. Still to come: an AI
path (#766): the browser editor has no
Assistant panel and no MCP or automation-socket control, only `openstitch.run` from code in the
page.

**Build** (the CLI version must match `wasm-bindgen` in Cargo.lock):

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129 --locked
cargo build -p openstitch-web --target wasm32-unknown-unknown --release --locked
wasm-bindgen --target web --no-typescript --out-dir web/pkg \
  target/wasm32-unknown-unknown/release/openstitch_web.wasm
```

**Host:** serve the `web/` folder (`index.html` and `pkg/`) from any static web server that sends
`.wasm` as `application/wasm`; for a local try, `python3 -m http.server --directory web` and open
`http://localhost:8000`. Opening `index.html` as a file does not work (browsers refuse to load
the module from `file://`). CI job `web` uploads the built page as the `openstitch-web` artifact
on every run.

**Published:** <https://cristianonescu.github.io/OpenStitch/> (#695, with the owner's OK).
`.github/workflows/pages.yml` builds the page from `main` whenever the crates, `web/` or the lock
file change (or by hand), writes the commit to `version.txt` beside it, and deploys it with
GitHub's Pages actions. Only its deploy job has `pages: write` and `id-token: write`, in the
`github-pages` environment, which accepts `main` only. It is its own workflow because
`release.yml` calls `rust.yml` without Pages permissions.

**Automation:** the page's `openstitch.run({command, params})` takes what the desktop editor's
automation socket takes (every engine command and the editor's `ui.*` commands, on the editor's
own undo history) plus the page's file commands `ui.openFile {name, base64}`, `ui.saveFile` and
`ui.exportFile {format}` (files as base64), and an engine command writing under `/web/save/`
becomes a download (skills/openstitch-embroidery/SKILL.md, "Web editor"). Before the editor has
started (the Node smoke test loads the module without a page) it is the window-less
`WebSession`.

## Known limits

- **MSI upgrades (#742).** Every build is version 0.1.0 with a new ProductCode, so the MSI's
  `MajorUpgrade` sets `AllowSameVersionUpgrades`: installing any OpenStitch MSI removes an
  installed OpenStitch of the same or an older version first (one entry in Settings ▸ Apps).
  `build-executables.yml` reads the built MSI's Upgrade table through Windows Installer's COM
  object and fails unless a row finds OpenStitch's UpgradeCode up to and including this version
  (`msidbUpgradeAttributesVersionMaxInclusive`) and `RemoveExistingProducts` is scheduled. The
  self-updater runs `msiexec /i` on the new MSI and relies on the same rows. Cleaning up a PC with
  older duplicate entries: RELEASING.md.
- **MSI validation.** `light` runs with `-sice:ICE50`: the advertised Start-menu shortcut's
  icon is `openstitch.ico` from the Icon table while its target is `openstitch.exe`, and ICE50
  wants the two extensions to match. Making the Icon entry from the exe would store the whole
  binary (about 30 MB) in the MSI's Icon table, so the `.ico` stays and ICE50 stays suppressed.
- **Not signed for distribution.** macOS builds are ad-hoc signed (Gatekeeper asks on first
  launch); Windows builds carry no Authenticode signature. Signing needs certificates from the
  owner (Apple Developer ID + notarisation; a code-signing certificate for Windows).
- **macOS double-click (Blocked).** Finder passes a document (double-click, *Open With*, a drop
  on the Dock icon) as an `odoc` Apple Event, not on the command line. Double-clicking a `.osd`
  or `.pes` starts OpenStitch but does not load the file; File ▸ Open works. Checked against the
  versions in `Cargo.lock` (eframe 0.37, winit 0.30.13, objc2 0.6 / objc2-app-kit 0.3):
  - winit has no file-open event. Its macOS docs say it will never set an application
    delegate and tell applications to register their own `NSApplicationDelegate` with
    `application:openURLs:` (`winit::platform::macos`, module docs).
  - Registering a delegate means defining an Objective-C class: objc2's `define_class!`
    (`declare_class!` before 0.6) requires `unsafe impl` blocks and `#[unsafe(method(...))]`
    attributes in the calling crate, and `#![forbid(unsafe_code)]` rejects them (they cannot
    be `allow`ed under `forbid`).
  - The other route, `NSAppleEventManager::setEventHandler_andSelector_forEventClass_andEventID`,
    is an `unsafe fn` and also needs a handler object of a defined class.
  - eframe 0.37 exposes no hook for this, and no maintained crate offers a safe wrapper that
    works with winit's event loop (tao, the Tauri fork of winit, has `Event::Opened`, but
    eframe cannot run on it).

  Unblocking needs one of: upstream support in winit/eframe (an `open_urls`-style event), or
  the owner allowing `unsafe` in one small, separately audited crate (a ~40-line objc2
  delegate that forwards the paths to the editor through a channel; the same exception as the
  Quick Look and Explorer thumbnailers below).
- **macOS application menu.** `OpenStitch.app` installs no application menu (the bold menu
  next to the Apple menu is empty): `main.rs` builds the event loop with winit's
  `with_default_menu(false)`. winit's default menu named its items after the process
  ("About openstitch", "Quit openstitch"; `CFBundleName` is not consulted) and its Quit item
  ended the process through `NSApplication terminate:`, which never reaches the window's close
  request, so Cmd+Q lost unsaved changes without the prompt (winit 0.30.13 / eframe 0.37, which
  has no hook into `applicationShouldTerminate:`; a delegate needs `unsafe` objc2 code, see
  *macOS double-click*). Without the menu the keys reach the editor: Cmd+Q and Cmd+W close
  through the unsaved-changes prompt, Cmd+M minimises (`EditorApp::macos_app_shortcuts`,
  listed in Help ▸ Keyboard Shortcuts). Not available: Cmd+H / Hide Others, Services and the
  menu's About (Help ▸ About OpenStitch remains). Quitting from the Dock icon's menu or at
  logout still terminates without the prompt. A native menu with the right names and a Quit
  item routed through the prompt needs a menu crate such as `muda` (safe API over objc2);
  not done yet.
- **Flatpak is not on Flathub.** The bundle comes from the `Flatpak` workflow; publishing on
  Flathub needs offline cargo sources (see [Flatpak](#flatpak)) and the owner's submission.
- File-manager thumbnails exist on Linux only; see below.

## Thumbnails

`openstitch-thumbnailer` (crate `openstitch-render`) writes a PNG of the stitches of an `.osd`
project or any machine file OpenStitch reads, cropped to the design on a transparent square:
`openstitch-thumbnailer -s SIZE INPUT OUTPUT.png`. Inputs over 64 MiB are refused; the readers
are the fuzzed `openstitch-formats` codecs. `stitchtool thumbnail FILE OUT.png [--size PX]
[--json]` runs the same renderer (`openstitch_render::thumbnail`, platform-neutral: a path in,
a PX × PX PNG out) for scripts and the Assistant; a malformed file is a one-line error with
exit code 2 (#557). The macOS Quick Look extension and the Windows Explorer handler (#669)
call this renderer too.

### Shared thumbnail interface

Every file-manager handler (Linux, macOS Quick Look, Windows Explorer) draws through one of
these two entry points. Keep them stable: add options, never rename or drop one.

- **Rust API** (`openstitch-render`): `thumbnail::load(path)` and `thumbnail::load_bytes(data,
  extension)` read a design (`osd` or any machine format OpenStitch reads; over 64 MiB refused)
  into stitches; `thumbnail::png(&stitches, size)` draws a `size` × `size` RGBA PNG (clamped to
  16-1024), cropped to the design on a transparent square.
- **Command line** (`openstitch-thumbnailer`):
  - `openstitch-thumbnailer [-s|--size N] INPUT OUTPUT.png` (the freedesktop form);
  - `openstitch-thumbnailer [-s|--size N] --stdin EXT`: the file's bytes on stdin, its format
    named by EXT (`pes`, `.PES`, `osd`; case and a leading dot are ignored), the PNG on stdout.
    For handlers that may not open the file by path (a sandboxed Quick Look extension).
  - Size defaults to 256. Exit 0 on success; 1 when the file cannot be read or drawn (a
    message on stderr starting `openstitch-thumbnailer: `, nothing on stdout); 2 on a usage
    error.
  - `crates/openstitch-render/tests/thumbnailer_cli.rs` pins these flags, outputs and exit
    codes.

| OS | State |
|---|---|
| Linux | Done: `packaging/linux/openstitch.thumbnailer` (`Exec=openstitch-thumbnailer -s %s %i %o`) for `application/x-openstitch-design` and `application/x-pes-embroidery`; `build-executables.yml` builds it next to the editor, `package.sh` ships it, `install.sh` installs it into `share/thumbnailers`, and the Linux job renders a thumbnail from the installed copy. |
| macOS | Done (#557): `OpenStitch.app/Contents/PlugIns/OpenStitchThumbnail.appex`, a Quick Look thumbnail extension (`QLThumbnailProvider`) for `.osd`, `.pes` and the machine formats the app's Info.plist declares (DST, JEF, VP3, PEC, XXX, HUS, VIP, SHV, PCS, PHC, U01, TBF, DSB, DSZ, KSM, ZXY). See *Quick Look extension* below. |
| Windows | Done (#669): `openstitch_explorer_thumbnails.dll`, an in-process COM server implementing `IInitializeWithStream` and `IThumbnailProvider` for `.osd` and every machine format OpenStitch reads (23 extensions). See *Explorer thumbnail handler* below. |
| PEN | Blocked: the format is undocumented (see FILE_FORMATS.md). |

The owner's decision (2026-11-06): the project is built in Rust, but a feature that needs
another tech stack on one OS may use it. Quick Look is therefore a small Swift shim; Explorer
can be a small native shim or a separately audited Rust crate the same way (#669). It is the
latter: a Rust COM server kept outside the workspace.

### Explorer thumbnail handler

`packaging/windows/thumbnails` is a separate Cargo project (excluded from the workspace, its own
`Cargo.lock`): COM needs `unsafe` code, which the workspace forbids. It builds
`openstitch_explorer_thumbnails.dll` with `windows-rs`:

- Explorer creates the class registered for the file's extension and hands it the file's bytes
  as a stream (`IInitializeWithStream`; the handler never opens the file itself, so Explorer can
  run it isolated). A stream has no file name, so each extension has its own class id,
  `{4F70656E-5374-6974-6368-54686D62xxxx}` + the extension's index in
  `openstitch_explorer_thumbnails::extensions()` (`osd`, then `FileType::ALL`; append only).
- `IThumbnailProvider::GetThumbnail(cx)` draws with `openstitch_render::thumbnail::load_bytes` and
  `png` (the same pixels as Quick Look and `openstitch-thumbnailer`) and returns a premultiplied
  32-bit bitmap with alpha. A file it cannot read, or one with no stitches, is an error, and
  Explorer keeps the type's own icon. Files over 64 MiB are not read.
- **Install**: `packaging/windows/openstitch.wxs` (component `ExplorerThumbnails`) installs the DLL
  and writes, per extension, `HKLM\Software\Classes\CLSID\{…}\InprocServer32` (the DLL,
  `ThreadingModel=Apartment`) and `HKLM\Software\Classes\.ext\ShellEx\{e357fccd-a995-4576-b01f-234630154e96}`
  (the class). **Uninstall** removes the DLL and every key. Nothing else registers it (no
  `regsvr32`, no self-registration).
- **Build and tests**: `build-executables.yml` builds it for the MSI; the Windows job in
  `rust.yml` runs its clippy and tests: every design in the ready-to-sew kit renders, the pixels
  equal `openstitch_render::thumbnail`'s, Explorer's call sequence (class factory, provider,
  stream, bitmap) yields a 256 × 256 32-bit bitmap without any registration, an unknown class is
  refused, and the installer registers every extension's class (the test reads `openstitch.wxs`).
- Not yet checked in a real Explorer window (that needs the MSI installed): an owner check on #669.

### Quick Look extension

`packaging/macos/quicklook` holds the extension, built outside the Cargo workspace by
`quicklook/build.sh` (Xcode's `swiftc`, arm64 + x86_64), which `bundle.sh` runs when given the
`openstitch-thumbnailer` binary:

- `ThumbnailProvider.swift` answers Finder's request: it reads the file Quick Look hands it and
  draws the PNG it gets back into Quick Look's context.
- `Renderer.swift` runs the Rust renderer bundled inside the extension,
  `Contents/MacOS/openstitch-thumbnailer -s PX --stdin EXT`, with the file's bytes on stdin and
  the PNG on stdout (a 10-second timeout; files over 64 MiB are refused). No drawing or format
  code is in Swift.
- The extension is sandboxed (`Thumbnail.entitlements`, as macOS requires for extensions); the
  helper carries `com.apple.security.inherit` (`Helper.entitlements`) and runs in the
  extension's sandbox, which is why it is given bytes rather than a path. Run on its own, the
  bundled helper is stopped by the system; the unsigned build is the same code.
- Its `QLSupportedContentTypes` are every type the app's Info.plist declares (filled in by
  `build.sh`). Generic extensions other programs use (.exp, .sew, .tap, .csd, .stx) are not
  declared; a type another installed program exports first keeps that program's identifier,
  and Finder then shows its icon instead.

CI (`build-executables.yml`, macOS) runs `quicklook/check.sh`: the extension's plist, types,
architectures and signatures; the Swift code path (`openstitch-quicklook-check`, built from
`Renderer.swift`) giving a 256 × 256 PNG for a kit design and a clean error for a malformed
file; and, on the throwaway runner only, registering the extension (`lsregister`,
`pluginkit -a`, which must list it) and asking `qlmanage -t` for a `.pes` thumbnail for at most
90 seconds. A runner has no login session and `qlmanage` may never answer there, so a missing
picture is a warning rather than a failure; the step has a 20-minute limit. That registration
is never run on someone's own Mac.

Signing: the bundle and the extension are ad-hoc signed (`codesign --sign -`), as before; CI
holds no certificate and none is added without the owner. Owner steps that CI cannot do:

1. Check Finder by hand once on the release build: move OpenStitch.app to /Applications, open
   it once, then look at a folder of `.pes`, `.dst` and `.osd` files in icon view (thumbnails
   appear; `qlmanage -r` resets Quick Look's cache if old icons stay). If none appear, check
   `pluginkit -m -i io.github.cristianonescu.openstitch.thumbnail`; macOS may refuse to load
   an ad-hoc signed extension from a downloaded (quarantined) app.
2. For distribution without that risk, sign the app and the extension with a Developer ID and
   notarize them (`xcrun notarytool`); this needs the owner's Apple Developer account and is
   not set up.

## Ready-to-Sew Designs on macOS

Help ▸ Ready-to-Sew Designs (and the Assistant's `ui.help {"item": "sewKit"}`) runs
`sew_kit::open_kit`: it finds the kit at `OpenStitch.app/Contents/Resources/sew` from the
running executable (its `manifest.json` marks it) and passes that folder to `open`, which
shows it in Finder; without a kit, or if `open` cannot start, it opens the kit on GitHub.

`build-executables.yml` checks this in the built bundle with `packaging/macos/check_sew_kit.sh`
(#319): it runs `OpenStitch.app/Contents/MacOS/openstitch --sew-kit`, which does what the menu
item does and prints the result, with a stand-in `open` first on `PATH` that records its
argument. The check passes when `open` receives exactly the bundle's `Contents/Resources/sew`,
and, with the kit moved away, when no `open` runs and the GitHub page is printed.

Not covered by the check: the click itself, and Finder coming to the front at that folder.
The owner checks this once, by hand: with the app in /Applications, click Help ▸ Ready-to-Sew
Designs (Finder opens at `OpenStitch.app/Contents/Resources/sew`); then rename `sew` and click
again (the GitHub page opens in the browser).
