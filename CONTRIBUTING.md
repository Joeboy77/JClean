# Contributing to JClean

Thanks for helping. JClean is free and open source, and it gets better every time someone teaches it about a cache it didn't know, or catches something it shouldn't have offered to clean.

This guide covers how to report a problem, how to add a rule (the most common and most useful contribution), and how to work on the code.

- [Ground rules](#ground-rules)
- [Reporting a bug](#reporting-a-bug)
- [Setting up](#setting-up)
- [Adding a rule](#adding-a-rule)
- [Working on the code](#working-on-the-code)
- [Pull requests](#pull-requests)

## Ground rules

JClean deletes files, and a single bug can destroy someone's work. Every change is judged against these rules first, before speed, convenience or elegance.

1. **One place deletes.** All deletion goes through `jclean_core::cleaner::execute`. Nothing else may call `remove_dir_all`, `remove_file` or the `trash` crate; a test (`tests/architecture.rs`) enforces this.
2. **Everything is re-checked right before it's cleaned.** The `SafetyGuard` canonicalizes each path, confirms it's inside a folder its rule may clean, refuses protected locations, and skips anything that changed since the scan.
3. **Links are never followed.** Not while scanning, not while deleting. Symlinks and Windows junctions are removed as links. Scans don't cross into other drives unless a rule explicitly opts in.
4. **File contents are never read** while scanning, only names, sizes and dates. Reading can download iCloud and OneDrive files that are only in the cloud.
5. **Tests never touch a real home folder.** They use temporary fixtures and pass the home path in through `Env`.
6. **Every cleanup supports a dry run** and is written to the local log.
7. **When you're unsure whether something is safe to delete, it's `review`, not `safe`.**

Be kind in issues and reviews. Assume good intent, and explain the _why_ behind a request.

## Reporting a bug

[Open an issue](https://github.com/Joeboy77/JClean/issues) with:

- your system and version (for example _macOS 15.2_, _Windows 11 24H2_, _Ubuntu 24.04_) and JClean's version (Settings → About);
- what you did, what you expected, and what happened;
- the rule involved if you know it (Developer mode shows each item's name, and the item's details show where it lives).

**If JClean offered to clean something it shouldn't have**, that's a safety bug and the top priority: please say so in the title. If the details are sensitive, use **Security → Report a vulnerability** on GitHub to report it privately instead.

Paths often contain your user name. Feel free to replace it with `me` before posting.

## Setting up

You'll need Rust (stable), Node.js 20.19 or later, pnpm 9 (`corepack enable` sets it up), and the [Tauri prerequisites](https://tauri.app/start/prerequisites/) for your system.

```sh
git clone https://github.com/Joeboy77/JClean.git
cd JClean
pnpm install
cd apps/desktop && pnpm tauri dev
```

To work without going near your real files, build a fake home folder and point the app or the CLI at it:

```sh
cargo run -p jclean-cli -- fixture /tmp/jc                 # a home with one of everything
cargo run -p jclean-cli -- scan --root /tmp/jc/root --home /tmp/jc/root/Users/tester
JCLEAN_DEV_ROOT=/tmp/jc pnpm tauri dev                     # the app, on the fixture (from apps/desktop)
```

The full product and technical spec is in [`docs/SPEC.md`](docs/SPEC.md). It's worth reading the section on the part you're changing.

## Adding a rule

Everything JClean detects is described in JSON under `rules/<os>/`, one file per ecosystem (`node.json`, `browsers.json`, `system.json`…). The files are built into the app, so adding a cache usually needs no Rust at all.

### An example

```json
{
  "id": "macos.python.pip-cache",
  "version": 1,
  "platforms": ["macos"],
  "audience": ["developer"],
  "category": "developer",
  "group": "Package caches",
  "labels": {
    "developer": "pip cache",
    "everyday": "Downloaded packages (pip)"
  },
  "description": {
    "what": "Copies of Python packages pip has downloaded.",
    "ifCleared": "pip downloads them again the next time you install or build. Nothing in your projects changes."
  },
  "icon": "package",
  "risk": "safe",
  "regenerates": true,
  "detect": { "kind": "fixed", "paths": ["{caches}/pip"] },
  "unused": { "basis": "newest-mtime", "minAgeDays": 0 },
  "cleanup": { "method": "delete" }
}
```

### The fields

| Field         | What it means                                                                                                                                                                                                                                   |
| ------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `id`          | `<os>.<ecosystem>.<name>`, lowercase with dashes, e.g. `windows.node.npm-cache`. Never reuse or rename an id: settings and history refer to it.                                                                                                 |
| `platforms`   | The one OS the file is for: `macos`, `windows` or `linux`.                                                                                                                                                                                      |
| `audience`    | `everyday`, `developer`, or both. Developer-only rules are hidden in Everyday mode.                                                                                                                                                             |
| `category`    | Colour on the disk map: `apps`, `developer`, `system`, `media`, `documents` or `other`.                                                                                                                                                         |
| `group`       | The heading the item is listed under. Reuse an existing group where one fits.                                                                                                                                                                   |
| `labels`      | A precise name for Developer mode and a plain one for Everyday mode.                                                                                                                                                                            |
| `description` | `what` it is, and what happens `ifCleared`. One or two plain sentences each.                                                                                                                                                                    |
| `icon`        | A [Lucide](https://lucide.dev/icons) icon name, from the ones already used in `rules/` (`package`, `globe`, `trash`, `scroll-text`…).                                                                                                           |
| `risk`        | See [Choosing the risk](#choosing-the-risk).                                                                                                                                                                                                    |
| `regenerates` | `true` if the app or tool recreates it on its own (a cache). `false` for anything that's someone's data.                                                                                                                                        |
| `detect`      | Where to look; see below.                                                                                                                                                                                                                       |
| `unused`      | How "last used" is worked out: `newest-mtime`, `project-activity` or `none`, plus `minAgeDays` before it can be pre-selected.                                                                                                                   |
| `cleanup`     | `method`: `delete` (permanent), `trash`, `command` (the tool's own cleanup) or `none` (information only). Optional: `keepRoot` to empty a folder but keep it, `requiresAdmin`, and a `fallback` method when the command's tool isn't installed. |
| `relatedApps` | Process names that must be closed first, e.g. `["Google Chrome"]` on macOS, `["chrome.exe"]` on Windows, `["chrome"]` on Linux.                                                                                                                 |
| `keep`        | Keep the newest _n_ of a set of versions: `{ "newest": 1, "groupBy": "name" }`.                                                                                                                                                                 |
| `docs`        | Optional link to the tool's own documentation about this folder.                                                                                                                                                                                |

### Detecting things

- **`fixed`**: known locations. `paths` can use globs (`{caches}/JetBrains/*`). Add `"eachChild": true` to list every entry inside a folder as its own item (with `exclude` for names to skip).
- **`project-artifact`**: build folders next to a marker file, e.g. `{ "markers": ["Cargo.toml"], "folders": ["target"] }`. Projects that are still active are raised to `review` automatically.
- **`query`**: files matching `names`, `olderThanDays` and `minBytes`, like old installers in Downloads.
- **`probe`**: asking a tool (Docker, `snap`, `journalctl`…). Probes are Rust code in `crates/jclean-core/src/probes.rs`; open an issue first if you need a new one.

Paths start with a token so they work on every machine:

| Token                           | macOS                           | Windows                       | Linux                                  |
| ------------------------------- | ------------------------------- | ----------------------------- | -------------------------------------- |
| `{home}`                        | `~`                             | `%USERPROFILE%`               | `~`                                    |
| `{caches}`                      | `~/Library/Caches`              | `%LOCALAPPDATA%`              | `$XDG_CACHE_HOME` (`~/.cache`)         |
| `{appSupport}`                  | `~/Library/Application Support` | `%APPDATA%`                   | `$XDG_DATA_HOME` (`~/.local/share`)    |
| `{logs}`                        | `~/Library/Logs`                |                               |                                        |
| `{temp}`                        | `$TMPDIR`                       | `%TEMP%`                      | `$TMPDIR` or `/tmp`                    |
| `{localAppData}`, `{appData}`   |                                 | `%LOCALAPPDATA%`, `%APPDATA%` |                                        |
| `{config}`, `{data}`, `{state}` |                                 |                               | the XDG config, data and state folders |
| `{env:NAME}`                    | any environment variable        |                               |                                        |

A path whose token doesn't exist on that system is simply skipped.

### Choosing the risk

| Risk      | Use it for                                                                                                                                                | Default cleanup                               |
| --------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------- |
| `safe`    | Regenerable and holds no one's data: package caches, build output of inactive projects. Only these are ever pre-selected.                                 | `delete`                                      |
| `review`  | Probably fine, but worth a look: things someone might still want, folders projects sometimes commit (`dist`, `build`), anything you're not certain about. | `delete` if it regenerates, otherwise `trash` |
| `caution` | May hold data that can't be recovered: device backups, Docker volumes. Always asks a second time. Never `delete`.                                         | `trash` or `command`                          |
| `info`    | Shown to explain where space goes; never cleaned (`"method": "none"`).                                                                                    | `none`                                        |

A few things are refused by validation no matter what: `caution` items that delete permanently, commands in user rule packs, and paths inside protected locations.

### Writing the words

- Sentence case and plain verbs. No jargon in the Everyday label.
- Say what's lost, honestly: "You'll need to download a model again before using it", not "Frees space".
- Keep the same verb through the flow ("Clean" → "Cleaned 4.2 GB").
- Sizes in decimal units (1 GB = 10⁹ bytes), as Finder and File Explorer show them.

### Checking your rule

```sh
cargo test -p jclean-core --test rules      # schema and safety validation of every rule file
cargo run -p jclean-cli -- rules            # is it listed?
cargo run -p jclean-cli -- scan             # does it find what you expect? (read-only)
```

If the rule has a command that needs administrator approval, it must also be added to that system's allowlist in `crates/jclean-core/src/platform/privileged/`, where a test checks every admin command in the rules against it.

## Working on the code

| Where                                 | What                                                                                                                                     |
| ------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| `crates/jclean-core`                  | The engine: scanning, sizing, rules, the SafetyGuard and the cleaner. No UI dependency.                                                  |
| `crates/jclean-core/src/platform`     | Everything that differs between macOS, Windows and Linux. OS-specific paths and behaviour live here and in the rule files, nowhere else. |
| `crates/jclean-cli`                   | A command-line harness for the engine.                                                                                                   |
| `apps/desktop/src-tauri`              | The Tauri shell: commands, events, tray, updater.                                                                                        |
| `apps/desktop/src`                    | The React interface.                                                                                                                     |
| `apps/web`                            | The download website (Astro).                                                                                                            |
| `packages/treemap`, `packages/design` | The disk map and design tokens, shared by the app and the website.                                                                       |

Conventions:

- **Rust:** `thiserror` for library errors, `anyhow` only in binaries. No `unwrap()` or `expect()` outside tests. Heavy work runs on background threads, never on Tauri's main thread. `unsafe` is only allowed in the few platform modules that call OS APIs, each with a `SAFETY:` comment.
- **TypeScript:** strict mode, no `any`. The IPC types in `apps/desktop/src/bindings.ts` are generated from Rust; after changing a command, run `cargo test -p jclean-desktop` and commit the updated file.
- **Interface:** the app is dark only. Colours come from the tokens in `packages/design`. Every animation respects reduced motion. Text meets WCAG AA contrast, and risk is never shown by colour alone.
- **Tests:** new behaviour comes with tests. Anything that decides what can be deleted comes with a test that it refuses what it should.

Before you push:

```sh
pnpm lint && pnpm typecheck && pnpm test
cargo fmt --check && cargo clippy --all-targets -- -D warnings
cargo test --workspace
```

Working on a Mac and touching Windows code? You can check it compiles without a Windows machine:

```sh
rustup target add x86_64-pc-windows-gnu && brew install mingw-w64
cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings
```

CI runs the full suite natively on macOS, Windows and Linux for every pull request.

## Pull requests

- Keep each pull request to one change, and each commit to one logical step.
- Use [conventional commits](https://www.conventionalcommits.org): `feat(rules): Poetry cache on Linux`, `fix(core): …`, `docs: …`.
- Describe what changed and why, and how you tested it. For a new rule, say where the folder comes from (the tool's documentation, or where you saw it on your own machine).
- A new dependency needs a one-line reason in the description. JClean's installer should stay small.
- Anything that changes what can be deleted gets a careful review. Please be patient with questions there.

By contributing, you agree that your contribution is licensed under the [MIT License](LICENSE).
