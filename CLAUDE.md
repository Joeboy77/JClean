# JClean

JClean is a free, open-source desktop app that finds the storage people don't know they're using (developer caches, stale build folders, app leftovers, old installers, logs, snapshots) and lets them review and clear it safely. There are no accounts or sign-up, and everything stays on the device.

The full product and technical spec is in `docs/SPEC.md`. Read it before starting any task, and re-read the relevant section before changing a subsystem. When this file and the spec disagree, the spec wins, and you should flag the conflict.

## Current target

- macOS first (Apple Silicon and Intel, shipped as one universal build), minimum macOS 12.
- Windows comes in phase 8, Linux in phase 9. Write core code so it stays portable. Keep all OS-specific paths and behavior inside rule files and the `platform` module, never scattered through the code.

## Stack

| Layer | Choice |
|---|---|
| Desktop shell | Tauri 2 |
| Core engine | Rust (stable), in a separate `jclean-core` crate |
| Frontend | React 19 + TypeScript (strict) + Vite |
| Styling | Tailwind CSS v4 with design tokens as CSS variables |
| Animation | Motion (`motion/react`, formerly Framer Motion) |
| State | Zustand |
| Long lists | TanStack Virtual |
| Treemap layout | `d3-hierarchy` (layout math only; React renders it) |
| Local data | SQLite via `rusqlite` (scan history), JSON for settings |
| Website | Astro + Tailwind, static, deployed to Cloudflare Pages |
| Package manager | pnpm workspaces + Cargo workspace |
| CI / releases | GitHub Actions + `tauri-action`, GitHub Releases, Tauri updater |

Don't add dependencies beyond these without a one-line justification in the PR/commit message. The app's own size matters here: a storage cleaner must stay small. Target installer size is under 15 MB.

## Repository layout

```
jclean/
  CLAUDE.md
  docs/SPEC.md
  crates/
    jclean-core/        # scanner, sizing, rules engine, cleaner, safety guard (no Tauri deps)
    jclean-cli/         # dev/test harness: `jclean-cli scan --dry-run`
  apps/
    desktop/
      src/              # React frontend
      src-tauri/        # Tauri shell: commands, events, tray, updater
    web/                # Astro download site
  packages/
    treemap/            # shared treemap layout + React component (app and website)
  rules/
    macos/*.json        # detection rules, one file per ecosystem
    windows/*.json
    schema/rule.schema.json
  .github/workflows/
```

## Commands

```
pnpm install
pnpm tauri dev                       # run desktop app (from apps/desktop)
pnpm --filter web dev                # run website
cargo test -p jclean-core
cargo run -p jclean-cli -- scan --dry-run --mode quick
cargo run -p jclean-cli -- fixture /tmp/jc   # safe fake home to scan/clean (prints the commands)
pnpm lint && pnpm typecheck && pnpm test
cargo test -p jclean-desktop         # also regenerates apps/desktop/src/bindings.ts (IPC types)
cargo clippy --all-targets -- -D warnings && cargo fmt --check
```

Keep this section updated if commands change.

## Non-negotiable safety rules

This app deletes files, and a single bug can destroy someone's work. These rules override speed, convenience and elegance.

1. All deletion goes through one function: `jclean_core::cleaner::execute(plan)`. Nothing else in the codebase may call `remove_dir_all`, `remove_file`, or the `trash` crate.
2. Before deleting, every path is canonicalized and re-checked against the `SafetyGuard` (spec §7): it must sit inside a root declared by the rule that matched it, must not be on the protected-path list, and must not have changed since the scan (size and mtime are rechecked).
3. Never follow symlinks while scanning or deleting. Never cross filesystem boundaries unless a rule explicitly opts in.
4. Tests never touch the real home directory. Use `tempfile` fixtures, and inject the home path through the `Env` abstraction.
5. Every cleanup supports dry-run and writes an entry to the local deletion log.
6. Never read file contents during scanning, only metadata. Reading can force iCloud placeholder files to download. The only exception is small metadata files the spec names explicitly (e.g. an iPhone backup's `Info.plist` for its device name), and never inside cloud folders.
7. If you're unsure whether something is safe to delete, the rule's risk is `review`, not `safe`.

## Code conventions

- Rust: `thiserror` for library errors, `anyhow` only in binaries. No `unwrap()`/`expect()` outside tests. Heavy work runs on `rayon`/background threads, never on the Tauri main thread.
- TypeScript: strict mode, no `any`. IPC types are generated from Rust with `specta`/`tauri-specta` so frontend and backend can't drift.
- UI copy: sentence case, plain verbs, and no jargon in Everyday mode (see spec §5.6). An action keeps the same name throughout the flow ("Clean" → "Cleaned 4.2 GB").
- Sizes are shown in decimal units (GB = 10⁹ bytes) to match what Finder and Windows Explorer show.
- Respect `prefers-reduced-motion` in every animation.
- Commit per logical step, using conventional commits (`feat(core): ...`).

## How to work through the spec

Build in the phase order from spec §13. Finish a phase's acceptance criteria before starting the next, and tell me when a phase is done so I can review. If something in the spec is technically wrong or there's a clearly better approach, raise it rather than silently diverging.
