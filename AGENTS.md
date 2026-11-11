# AGENTS.md

Instructions for coding agents other than Claude Code (Codex, Cursor, Gemini CLI, Aider, …). Claude Code reads `CLAUDE.md` directly and can ignore this file.

The project's agent knowledge lives in Claude Code's layout. Nothing here repeats it; this file says what to read, because your harness won't load it for you.

## Before any task

1. Read `CLAUDE.md`: architecture, commands, CI gates, branching and commit rules, the silent gotchas. Everything in it applies to you.
2. Read `.claude/rules/code-standards.md`: always-on code rules (lints, error handling, testing).
3. Read `.claude/skills/codebase-map/SKILL.md`: which directory and entry files a task goes to. Use it instead of re-exploring the tree.

## Before editing a file

Claude Code injects the path-scoped rules in `.claude/rules/` when a matching file is read; your harness does not. Run

```bash
python3 .claude/skills/codebase-map/rules_for.py <every file you plan to edit>
```

and read each rule file it prints. Those files hold the invariants that break silently (engine lock, queue indices, config routing, …).

## Skills

`.claude/skills/<name>/SKILL.md` are step-by-step procedures. Read the matching one in full when its trigger fits:

- adding or changing a feature (view, setting, hotkey, menu entry, playback behaviour): `new-feature-checklist`
- committing: `commit`; changelog: `unreleased`; releases and version bumps: `package` (only when the owner asks)
- anything platform-specific or failing only on macOS: `macos-port` first, then `macos-audio`, `macos-desktop-integration` or `macos-bundle`
- auditing these docs against the code: `sync-rules`

## Hard requirements (details in CLAUDE.md)

- All four gates pass before handing off: `cargo +nightly fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo build --release`.
- No new crates without the owner's OK.
- Never edit `reference-*/`. Never revert uncommitted changes you did not make; stage your own files by name.
- Commits: Conventional Commits, no AI attribution trailers.
