---
name: codebase-map
description: Where things live in nokkvi — a task-to-directory map plus two lookup scripts (per-file module docs, which rule files cover a path). Use at the start of a task, before grepping or listing directories to find the code to change.
---

# Codebase map

Read this instead of re-exploring the tree. It routes a task to its directory and entry files; the per-file inventory is the `//!` module doc at the top of each file (canonical, so this map stays coarse and does not go stale). Architecture, invariants and commands are in CLAUDE.md; the per-area rules are in `.claude/rules/`.

## Two lookups

```bash
.claude/skills/codebase-map/modules.sh src/update data/src/audio   # file | first //! line, per dir (no args = whole workspace)
python3 .claude/skills/codebase-map/rules_for.py <path>...         # the rule files that apply to each path
```

Run `rules_for.py` on every file you plan to edit and read what it prints. Claude Code auto-injects those rules on the first Read, but not after /compact until a matching file is read again, and other agents (AGENTS.md) never get them.

## Where a task goes

The UI crate is `src/`, the iced-free backend is `data/src/`, the CLI-control wire layer is `nokkvi-ipc/src/`.

| Task | Start here | Then |
|------|-----------|------|
| App state, startup, subscriptions, `shell_task` | `src/main.rs` (`Nokkvi`) | `src/state/` (state grouped by domain, one file each) |
| Root message enum, menus enum | `src/app_message.rs` | dispatcher `src/update/mod.rs` |
| Top-level layout (nav, panes, modals stack) | `src/app_view.rs` | `src/widgets/nav_bar.rs`, `side_nav_bar.rs`, `now_playing_sidebar.rs` |
| A library view (Albums, Artists, Genres, Playlists, Songs, Queue) | `src/views/<name>/{mod,update,view}.rs` | its handler `src/update/<name>.rs`, tests `src/update/tests/<name>.rs` |
| Single-file views | `src/views/{harbour,radios,similar,login,browsing_panel}.rs` | handler of the same name in `src/update/` |
| Shared slot-list machinery | `src/widgets/slot_list{,_view,_page}.rs`, `base_slot_list_layout.rs` | `src/views/expansion.rs`, `impl_expansion_update!` in `src/views/mod.rs`, `src/update/slot_list.rs` |
| Load results, paging | `src/update/loader_target.rs` | `data/src/types/paged_buffer.rs` |
| Shared handler helpers (play guard, ratings, prefetch) | `src/update/components/` | — |
| Playback transport, modes, seek | `src/update/playback.rs`, `player_bar.rs` | backend `data/src/backend/playback_controller.rs` |
| Queue logic (order, shuffle, consume, navigation) | `data/src/services/queue/` | `data/src/backend/{queue,queue_orchestrator}.rs`, UI `src/update/queue.rs` |
| Audio engine, crossfade, decoder, EQ, output | `data/src/audio/` (`engine.rs`, `renderer.rs`, `decoder.rs`) | rules `audio-engine.md`; macOS sink → `macos-audio` skill |
| Navidrome / Subsonic API call | `data/src/services/api/<endpoint>.rs` | service in `data/src/backend/`, sort strings `src/views/sort_api.rs` |
| Live server events (SSE) | `src/services/navidrome_sse.rs` | parser `data/src/services/navidrome_events.rs`, refresh `src/update/library_refresh.rs` |
| A user setting | `data/src/services/settings_tables/<tab>.rs` (`define_settings!`) | types `data/src/types/player_settings/`, UI `src/views/settings/items_<tab>.rs`; skill `new-feature-checklist` |
| config.toml / theme file writes | `src/config_writer.rs`, `data/src/services/toml_settings_io.rs` | hot reload `src/update/config.rs`, paths `data/src/utils/paths.rs` |
| Persistence (redb) | `data/src/services/state_storage.rs` | keys `storage_keys.rs` |
| Hotkey | action enum `data/src/types/hotkey_config/action.rs` | handlers `src/update/hotkeys/`, key capture `src/hotkeys/` |
| Theme colors, radii, fonts, styles | `src/theme/` | theme files `themes/*.toml`, loader `data/src/services/theme_loader.rs` |
| Visualizer (bars/lines, shaders, MilkDrop) | `src/widgets/visualizer/` | FFT `data/src/audio/spectrum.rs`, config `src/visualizer_config.rs`; rules `visualizer.md` |
| Lyrics | `src/update/lyrics.rs`, `src/state/lyrics.rs` | `data/src/types/lyrics.rs`, `data/src/services/lyrics_source.rs`, widget `lyrics_viewport.rs` |
| Artwork caches and prefetch | `src/state/artwork.rs`, `src/update/artwork.rs` | `src/update/components/artwork_prefetch.rs`, URLs `data/src/utils/artwork_url.rs` |
| Theater Mode | `src/update/theater.rs`, `src/state/theater.rs` | rules `ui-window.md` |
| Modals (info, EQ, about, pickers, trawl) | `src/update/modals.rs` (registry) | widget `src/widgets/<name>_modal.rs`, handler `src/update/<name>_modal.rs` |
| Context / overlay menus | `src/widgets/context_menu.rs`, `menu_chrome.rs` | `src/update/menus.rs` (`SetOpenMenu`) |
| Per-frame animation tick | `src/update/frame.rs` | — |
| Scrobbling | `src/update/scrobbling.rs`, `src/state/scrobble.rs` | radio: `data/src/services/radio_scrobble/` |
| MPRIS / tray / notifications | `src/services/{mpris,tray,notifications}.rs` | handlers `src/update/{mpris,tray,notifications}.rs`; macOS → `macos-desktop-integration` skill |
| `nokkvi <verb>` CLI control | `nokkvi-ipc/src/` | UI side `src/services/ipc.rs`, `src/update/ipc.rs` |
| Icons | `assets/icons/` + `assets/icons-phosphor/` | `NAME_MAP` in `src/embedded_svg.rs`, `build.rs` |
| Handler tests | `src/update/tests/<area>.rs` | `test_app()` in `src/test_helpers.rs` |
| CI, release, audit | `.github/workflows/{ci,release,audit}.yml` | `/package` skill |

## Not project code

`reference-*/` (cloned upstream repos: read, never edit), `target/`, `tmp/`, `local/`, `.venv/`.

## Keeping this map honest

Rows name directories and entry files only. When a move or rename breaks a row, fix it in the same commit; `/sync-rules` audits this file with the other skills.
