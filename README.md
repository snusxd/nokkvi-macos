<p align="center"><img src="assets/nokkvi_logo_readme.svg" width="96" alt="Nokkvi logo" /></p>

<h1 align="center">Nokkvi</h1>

<p align="center">
  <a href="https://github.com/snusxd/nokkvi-macos/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/snusxd/nokkvi-macos/ci.yml?branch=macos&label=CI" alt="CI status" /></a>
  <a href="https://github.com/snusxd/homebrew-nokkvi-macos"><img src="https://img.shields.io/github/v/release/snusxd/nokkvi-macos?label=brew&sort=semver" alt="Homebrew version" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/github/license/snusxd/nokkvi-macos" alt="License" /></a>
</p>

> [!NOTE]
> This is the **macOS fork** of [f-o-o-g-s/nokkvi](https://github.com/f-o-o-g-s/nokkvi). It tracks upstream releases (tagged `vX.Y.Z-macos.N`) and adds macOS support along with its own changes. Linux users should use [f-o-o-g-s/nokkvi](https://github.com/f-o-o-g-s/nokkvi) upstream.

A native Rust/Iced client for [Navidrome](https://www.navidrome.org/) music server. Named after Old Norse *nökkvi*, a small, humble boat.

<img src="assets/nokkvi_demo.webp?v=5" width="100%" alt="Nokkvi theme and visualizer demo" />

*MilkDrop presets, Theater Mode, and theme swaps, tiled and floating.*

> **⚠️ AI-Generated Project**
>
> Upstream Nokkvi was written by AI (primarily Claude) with the author's direction, and this macOS fork is likewise AI-maintained. You'll probably find patterns in here that experienced Rust developers would do differently. If you spot something that could be better, issues and PRs are welcome.

**Platform:** macOS 11+ (Apple silicon and Intel). Tested with Navidrome v<!-- navidrome-version -->0.64.2<!-- /navidrome-version -->.

**Network:** Built and tested on LAN. Nokkvi streams your original files by default, so FLAC over a slow remote connection can keep stopping to buffer. For remote listening, pick a smaller copy in Settings > Playback > Streaming Quality, and you can still seek in it as long as your server supports that (Navidrome 0.51 and newer do). A transcoding forced on Nokkvi's player in Navidrome under Settings > Players still plays fine, but you can't seek within those tracks.

## 📖 Documentation

Full guides, config reference, keyboard shortcuts, theming, and visualizer details are at the upstream **[Nokkvi docs site](https://f-o-o-g-s.github.io/nokkvi-docs/)** *(written for upstream Linux, so paths and Linux-only features like PipeWire or MPRIS differ on macOS)*.

## Highlights

- **CoreAudio** playback: gapless transitions, crossfade, a 10-band EQ, and ReplayGain *(no PipeWire engine or bit-perfect output on macOS)*.
- GPU visualizer (`bars`, `lines`, `scope`) with beat glow, bloom, reflection, and CRT effects, plus a **MilkDrop** mode with a live-preview preset picker and nokkvi's own theme-colored presets.
- **Theater Mode** (F11): the playing track's cover, visualizer, and lyrics fill the window.
- **Smart playlists** (Navidrome 0.61+) built in-app with a rules editor and server-evaluated previews; also imports `.nsp`.
- **Trawl** mix builder: blend artist, album, song, genre, and playlist seeds into one mix and save it as a playlist.
- Scriptable from the shell: `nokkvi <verb>` controls the running player (playback, volume, ratings, queue).
- **23 built-in themes** (default **Svalbard**), custom `.toml` themes that hot-reload, and optional accent and visualizer colors taken from the playing album cover.
- Adaptive layout: a width-adaptive player bar and a **slot-paginated list** that fits a whole number of rows to the window.
- Plus the essentials: browse albums (with notes from the server), artists (discographies grouped by release type), songs, genres, playlists, and radio; a **Harbour** home screen with a one-press Instant Mix (songs like what's playing, or like a few of your recent plays); Autoplay, which tops up a low queue with similar songs; persistent queue with drag-and-drop; star ratings; lyrics synced by line or by word; similar songs, plus similar artists from ListenBrainz and Deezer; five seek bar styles (one draws the waveform); server queue sync; scrobbling; **Now Playing / Control Center** integration and **media keys**; and full keyboard control with **Cmd** shortcuts. *(Menu-bar tray icon and desktop notifications are not implemented on macOS yet.)*

### Fork additions

- **Cover Beat visualizer:** the now-playing artwork swells on each kick drum and settles back (grows past its edges in Theater Mode, zooms inside its frame in Queue and Radios; Subtle or Punchy strength in Settings > Interface > Artwork Column).
- **Discord Rich Presence:** optional Discord App ID under Settings > Playback > Discord displays your current track on your Discord profile, with album art when Navidrome is reachable from the internet.
- **macOS-native shortcuts:** bindable `Cmd` modifier with `⌃⌥⇧⌘` labels, `Cmd` defaults in place of `Ctrl`, `Cmd+click` multi-selection, and protection against binding system shortcuts like `⌘Q`, `⌘C`, or `⌘Tab`.
- **Layout-independent hotkeys:** letter, punctuation, and number-row bindings work on non-Latin (e.g. Russian) and AZERTY keyboard layouts.
- **Lower CPU usage:** idle visualizers and unfocused Harbour scenes pause automatically, Harbour caps at 60 fps on high-refresh displays, paused long titles rest until hovered, and playback avoids full-window redraws.
- **macOS app icon:** Nokkvi icon set on a dark graphite plate.
- **Now Playing sidebar:** a column with the playing track's title, artist and album card (Settings > Interface, or `Cmd+B`).
- **Font Size and Font Weight** settings that scale and thicken all text.
- **Smooth trackpad scrolling** that glides by pixels and settles onto the row grid.

### Non-Goals

Nokkvi is a fast, keyboard-driven music player, not a Navidrome admin panel. For now it leaves out server administration (library scans, user management), podcasts, jukebox mode, bookmarks, public sharing links, and offline downloads.

## Install

### Homebrew

```bash
brew tap snusxd/nokkvi-macos
brew trust snusxd/nokkvi-macos   # Homebrew 7+ loads third-party taps only once trusted
brew install --cask nokkvi-macos
```

To update later:

```bash
brew upgrade --cask nokkvi-macos
```

### Download

Grab the `.dmg` or `.zip` for your Mac (`aarch64-apple-darwin` for Apple silicon, `x86_64-apple-darwin` for Intel) along with its `.sha256` from [Releases](https://github.com/snusxd/nokkvi-macos/releases), verify the checksum, and copy `Nokkvi.app` to `/Applications`:

```bash
shasum -a 256 -c nokkvi-vX.Y.Z-macos.N-aarch64-apple-darwin.dmg.sha256
```

**First launch (Gatekeeper):** `Nokkvi.app` is ad-hoc signed and not notarized. On first launch, either clear the quarantine attribute:

```bash
xattr -dr com.apple.quarantine /Applications/Nokkvi.app
```

or right-click `Nokkvi.app` in Finder and choose **Open**.

### Build from source

Requires Xcode Command Line Tools (`xcode-select --install`), Rust via [rustup](https://rustup.rs/) (stable, plus nightly for `rustfmt`), and `cmake` (for the bundled `libopus`):

```bash
brew install cmake
scripts/bundle-macos.sh --install
```

`scripts/bundle-macos.sh --install` builds a release binary, wraps it in an ad-hoc-signed `Nokkvi.app`, and installs it to `~/Applications/Nokkvi.app`. Rerun it after every rebuild.

## CLI

`nokkvi <verb>` controls a running instance from the terminal (playback, volume, ratings, queue, modes) against the bundled binary.

The Homebrew cask links `nokkvi` for you. If you installed from a DMG/ZIP, symlink the bundled binary onto your `PATH`:

```bash
ln -s /Applications/Nokkvi.app/Contents/MacOS/nokkvi /opt/homebrew/bin/nokkvi
```

*(For a source install, point the symlink at `~/Applications/Nokkvi.app/Contents/MacOS/nokkvi`.)*

## Paths & Troubleshooting

- **Config, state, and logs:** `~/Library/Application Support/nokkvi/` (`config.toml`, `themes/`, `sfx/`, `milkdrop/`, `app.redb`, `nokkvi.log`, `lyrics/`)
- **Caches:** `~/Library/Caches/nokkvi/`

Filing a bug? See [debug logging](CONTRIBUTING.md#debug-logging) (on macOS the log file is `~/Library/Application Support/nokkvi/nokkvi.log`).

More in the upstream docs *(Linux-oriented in places)*: [Installation](https://f-o-o-g-s.github.io/nokkvi-docs/guides/installation/) · [Connecting to Navidrome](https://f-o-o-g-s.github.io/nokkvi-docs/guides/navidrome/) · [Configuration](https://f-o-o-g-s.github.io/nokkvi-docs/reference/config/) · [Keyboard shortcuts](https://f-o-o-g-s.github.io/nokkvi-docs/reference/hotkeys/)

Build setup and contributor workflow (including the **nightly** rustfmt requirement) are in [CONTRIBUTING.md](CONTRIBUTING.md).

## Inspirations

- **[rmpc](https://github.com/mierak/rmpc)**: The upstream author's previous daily driver, a terminal MPD client.
- **[Feishin](https://github.com/jeffvli/feishin)**: Referenced heavily for Navidrome API coverage and enums.
- **[mpd](https://github.com/MusicPlayerDaemon/MPD)**: Shaped the queue and consume logic.
- **[fooyin](https://github.com/fooyin/fooyin)**: Referenced for the upstream native PipeWire implementation.
- **[cava](https://github.com/karlstav/cava)**: The visualizer DSP is a Rust port of cavacore, and the Monstercat smoothing comes from cava.c.
- **[OjoDrop](https://github.com/sho-run/ojodrop)** and **[Butterchurn](https://github.com/jberg/butterchurn)**: MilkDrop mode runs OjoDrop's Rust engine on presets from Butterchurn's converted pack.
- **[StepMania](https://github.com/stepmania/stepmania)**: Inspired the MusicWheel-style slot list and the roulette wheel.
- **Vim**: Inspired the color schemes and keyboard-first approach.
