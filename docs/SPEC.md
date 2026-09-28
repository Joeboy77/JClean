# JClean — product and technical specification

Version 1.0 · Owner: Joe Acheampong · Status: ready to build

## 1. Overview

Laptops fill up with storage that nobody chose to keep: package manager caches, `node_modules` in projects untouched for a year, Xcode simulators, Docker images, old IDE versions, installers sitting in Downloads, logs, local snapshots. macOS lumps most of it into "System Data", and Windows scatters it across hidden folders. Users can't see it, so they can't clear it.

JClean scans the machine, shows every place storage is going (with hidden and unused items called out), explains each one in plain language, and lets the user decide what to clear. It is safe by default: anything that isn't clearly regenerable needs review, and nothing is deleted without the user confirming.

### Goals

- Show where storage is going, visually and as a list, within seconds of opening the app.
- Find reclaimable space other tools miss, especially developer caches and stale project build folders.
- Make cleaning safe enough that an everyday user can trust the defaults.
- Deliver a polished, calm, animated interface on the level of Proton VPN's desktop app, which is the visual reference.
- Stay small, fast and fully offline. No accounts, no telemetry, no network calls except the optional update check.

### Non-goals for v1

- Duplicate file finding, app uninstallation, and scheduled auto-cleaning (planned for later, see §17).
- Memory/RAM "optimization", startup item management, antivirus. JClean is about disk storage only.
- Cloud storage cleanup (iCloud, OneDrive, Dropbox contents).

## 2. Users and modes

Anyone can use JClean without signing up. On first launch they choose a mode, and they can switch at any time in Settings or from the mode switch in the sidebar.

**Everyday mode** is for people who just want space back. It shows general categories only (app caches, browser caches, old downloads, logs, Trash, large files, iPhone backups, system snapshots). All labels are plain language ("Leftover files from apps", not "~/Library/Caches"). Paths are hidden behind a "Show location" disclosure.

**Developer mode** shows everything in Everyday mode plus the developer catalog (package manager caches, build folders, SDKs, simulators, containers, IDE caches). Rows show real paths and tool names, and project build folders are grouped by project.

Mode changes which rules are active (see the `audience` field in §6) and which label set is used. It never changes safety behavior.

## 3. Architecture

```
┌────────────────────────── Tauri app ──────────────────────────┐
│  React UI (WebView)                                           │
│   Zustand stores ── generated IPC types (tauri-specta)        │
│        ▲ events (scan progress, results)   │ commands          │
│        │                                   ▼                   │
│  src-tauri: thin command layer, tray, updater, window mgmt    │
│        │                                                       │
│        ▼                                                       │
│  jclean-core (pure Rust library, no Tauri dependency)         │
│   ├─ env        home dir, OS, env vars (injectable for tests) │
│   ├─ rules      load + validate JSON rules, match paths       │
│   ├─ scanner    parallel walker, sizing, project detection    │
│   ├─ probes     run tool queries (docker system df, etc.)     │
│   ├─ planner    turn user selection into a CleanPlan          │
│   ├─ safety     SafetyGuard, protected paths, re-verification │
│   ├─ cleaner    the ONLY module that deletes                  │
│   ├─ history    SQLite: scans, cleanups, deletion log         │
│   └─ platform   macOS / Windows / Linux specifics             │
└────────────────────────────────────────────────────────────────┘
```

Key decisions:

- **`jclean-core` has no Tauri dependency.** It can be unit tested in isolation and driven by `jclean-cli`, and a CLI release later is cheap.
- **Rules are data, not code.** Detection lives in `rules/<os>/*.json`, embedded into the binary at build time with `include_str!`. User-defined rules load from the app config directory. Adding a new cache location should never require Rust changes.
- **Streaming results.** Scans push progress and partial results to the UI through a Tauri `Channel`, so the list and map fill in live rather than appearing all at once at the end.
- **Cancellation.** Every long operation takes a cancellation token, and the UI can cancel any scan instantly.

Core crates: `jwalk` (parallel directory walking), `rayon`, `trash`, `rusqlite` (bundled), `serde`/`serde_json`, `jsonschema` (rule validation), `globset`, `sysinfo` (running processes, disk info), `thiserror`, `tracing`.

## 4. Scanning and sizing

### 4.1 Scan modes

**Quick scan** checks only the locations named by active rules, plus tool probes. It should finish in under 5 seconds on a typical machine and runs automatically on app launch (configurable).

**Full scan** walks the whole home directory (and system locations if Full Disk Access is granted) to build the disk map, find large files, and discover project build folders anywhere. It runs on demand from the Scan button, shows progress, and can be cancelled.

### 4.2 Sizing rules

Getting sizes right is what makes the app trustworthy, so:

- Report **allocated size** (`st_blocks × 512` on Unix; `GetCompressedFileSizeW` on Windows), not logical length. Sparse and compressed files would otherwise be overstated.
- **Hard links count once.** Track `(device, inode)` pairs per scan. This matters for pnpm stores and Homebrew.
- **APFS clones** can't be detected cheaply. Accept slight overcounting, and word reclaimable estimates as "up to" in the UI where a rule is flagged `mayShareBlocks` (e.g. copied Xcode data).
- **Never follow symlinks.** Report the symlink itself at its own size.
- **Stay on one filesystem.** Don't cross mount points (external drives, network shares, `/Volumes`) unless the user explicitly adds one.
- **iCloud / cloud placeholders.** Only call `stat`/`lstat`, never open files. Skip the contents of `~/Library/Mobile Documents` and `~/Library/CloudStorage` for the disk map beyond a total size.
- **Permission errors** are recorded and summarized ("12 folders couldn't be read — grant Full Disk Access to include them"), not treated as failures.

### 4.3 Disk capacity figures

Show three numbers for the main volume: total capacity, used, and available. On macOS, "available" uses `NSURLVolumeAvailableCapacityForImportantUsageKey`, which includes purgeable space and matches what Finder shows. Also show purgeable space separately, with an explanation that macOS frees it automatically when needed.

### 4.4 "Unused" detection

Last-access times are unreliable on macOS and Windows, so "unused" is based on:

1. For project build folders: the most recent of (a) the last git commit date in the project, via reading `.git/logs/HEAD` mtime or running `git log -1 --format=%ct` if git is installed, and (b) the newest mtime among the project's source files, sampled from the top two directory levels, excluding the build folders themselves.
2. For caches and other locations: the newest mtime inside the location, sampled with a cap of 10,000 entries per location.
3. A project is **inactive** when that date is older than the threshold (default 90 days, adjustable in Settings: 30 / 60 / 90 / 180 / 365).

Each item shows "Last used 7 months ago" style wording.

### 4.5 Project detection

A folder is a project when it contains a marker file. Build folders inside it are candidates only if they sit next to the right marker:

| Marker | Build/cache folders |
|---|---|
| `package.json` | `node_modules`, `.next`, `.nuxt`, `dist`, `build`, `.turbo`, `.parcel-cache`, `.svelte-kit`, `.angular/cache`, `.expo` |
| `Cargo.toml` | `target` |
| `pom.xml` | `target` |
| `build.gradle(.kts)` | `build`, `.gradle` |
| `pyproject.toml`, `requirements.txt`, `setup.py` | `.venv`, `venv`, `__pycache__`, `.pytest_cache`, `.mypy_cache`, `.tox` |
| `Podfile` | `Pods` |
| `pubspec.yaml` | `.dart_tool`, `build` |
| `*.xcodeproj` / `*.xcworkspace` | `build` |
| `go.mod` | (none; Go uses the global cache) |
| `*.csproj` / `*.sln` | `bin`, `obj` |
| `CMakeLists.txt` | `build`, `cmake-build-*` |

`dist` and `build` are `review` risk, because some projects commit them or use them for non-generated files. `node_modules`, `target`, `.venv` and similar are `safe` once the project is inactive, and `review` when the project is active.

Scan roots for project discovery default to the home directory, excluding `~/Library`, `~/.Trash`, and cloud folders. Users can add or remove roots in Settings.

## 5. Interface and experience

The reference is the Proton VPN desktop app: a fixed control sidebar on the left and a large visual canvas on the right, with the same window collapsing to just the sidebar when narrowed. JClean follows that structure. The canvas shows a map of the disk instead of a world map, the status card shows disk health instead of a connection, and the list shows storage categories instead of countries.

Quality bar: this should feel like a native premium app, not a web page in a window. That means no layout jumps, no spinners where a skeleton or progressive fill works, instant response to every click, and motion that explains what changed.

### 5.1 Window modes

| Mode | Size | Contents |
|---|---|---|
| Expanded (default) | min 1080 × 680, default 1280 × 800 | Sidebar (380 px) + disk map canvas |
| Compact | 380 × 760 (height resizable, min 600) | Sidebar only |

- The collapse/expand button sits in the sidebar filter row (same position as Proton's `>|` icon). Pressing it animates the window width and slides the canvas out (or in). The sidebar never moves or reflows.
- Resizing the window below 760 px wide switches to compact automatically, and widening past it restores expanded.
- The last mode, size and position are remembered.
- macOS: native traffic lights, transparent title bar (`titleBarStyle: Overlay`), and sidebar vibrancy (`NSVisualEffectMaterial.Sidebar`) where supported, falling back to a solid surface.

### 5.2 Expanded layout

```
┌─────────────── Sidebar 380px ───────────────┬───────────────── Canvas ────────────────────────┐
│ ● ● ●                                       │              ╭───────────────────╮   JClean    │
│ ┌─────────────────────────────────────────┐ │              │  Ready · 38.6 GB  │   [–]──[+]  │
│ │ Macintosh HD                            │ │              │    can be freed   │             │
│ │ 142.3 GB free of 494.4 GB               │ │              ╰───────────────────╯             │
│ │ ▓▓▓▓▓▓▓▓▓▓▓▓▓▒▒▒▒▒░░░░░░░░░░░           │ │  Home › Library › Developer                    │
│ │ Apps  Dev  System  Media  ░ Free        │ │ ┌──────────────┬────────┬────────────────────┐ │
│ │ [ Clean 38.6 GB ]   [ Scan again ]      │ │ │              │        │                    │ │
│ └─────────────────────────────────────────┘ │ │  Xcode       │ Docker │  node_modules      │ │
│  Categories        Rules                    │ │  41.2 GB     │ 18 GB  │  (23 projects)     │ │
│ [Dev] [System] [Large] [Safe only] [>|]     │ │              │        │  12.9 GB           │ │
│ ┌ Search storage… ────────────────────────┐ │ ├──────────────┼────┬───┴────────┬───────────┤ │
│  Safe to clean (14)             22.4 GB  ⓘ  │ │ Photos       │npm │  Gradle    │  …        │ │
│  ☑ ⧉ npm cache                  6.1 GB  ›  │ │ Library      │    │            │           │ │
│  ☑ ⧉ Xcode build data           9.8 GB  ›  │ └──────────────┴────┴────────────┴───────────┘ │
│  Needs review (9)               14.8 GB  ⓘ  │                                                │
│  ☐ ⧉ iOS simulators             7.3 GB  ›  │                              ┌───────────────┐ │
│  Caution (3)                     1.4 GB  ⓘ  │                              │ legend / hover│ │
│  ☐ ⧉ iPhone backups             1.4 GB  ›  │                              └───────────────┘ │
└─────────────────────────────────────────────┴────────────────────────────────────────────────┘
```

**Sidebar, top to bottom:**

1. **Status card.** Volume name, "X GB free of Y GB", and a segmented capacity bar coloured by category (Apps, Developer, System, Media, Documents, Other, Free). The reclaimable portion of each segment has a slow shimmer after a scan. The card background is a soft gradient whose hue shifts with disk health: violet when healthy (under 80% used), warmer amber at 80–90%, rose above 90%. Two buttons below (same shape and position as Proton's Disconnect / Change server):
   - Primary button, whose label follows state: "Scan" → "Scanning… 38%" (click to cancel) → "Clean 38.6 GB" (the selected total) → "Cleaning…" → "Scan again".
   - Secondary button: "Scan again" / "Full scan" / "Review selection", depending on state.
2. **Tabs.** "Categories" (the storage list) and "Rules" (browse all detection rules, toggle rules on/off, add custom folders). Animated underline indicator.
3. **Filter row.** Icon toggle buttons with tooltips: Developer, System, Large files, Safe only, and the collapse/expand button. Active filters show a count badge, like Proton's shield counter.
4. **Search** across item names, paths and tool names.
5. **Grouped list** in three risk sections, each with a count, section total and an info tooltip explaining the risk level: "Safe to clean", "Needs review", "Caution". Items with no reclaimable space (for example, system-owned areas shown only for understanding) appear in a fourth collapsed section, "For your information".

**List row anatomy:** checkbox · category icon · name (plus a secondary line: "Last used 7 months ago · 23 projects") · size, right-aligned with tabular figures · chevron to expand. Expanding reveals child items (for example, each project's `node_modules` with its project path and last-used date), each individually selectable. Hovering a row highlights its cell on the map, and a thin connector line animates from the row edge to the cell, echoing Proton's line from home to server. Clicking the row name opens the detail drawer.

**Detail drawer** (slides over the canvas from the right in expanded mode, full-sidebar sheet in compact mode): what it is in plain language, what happens if it's cleared (for example, "npm downloads these packages again the next time you install them"), whether it regenerates, the risk level and why, locations (with a "Show in Finder" button), size breakdown, last used, and the cleanup method (moved to Trash, deleted, or the tool's own cleanup command).

**Canvas:**

- **Disk map**: a squarified treemap of the scanned area. Cells are sized by allocated bytes and coloured by category with muted fills. Reclaimable cells are brighter and carry a fine diagonal hatch, so the eye goes straight to what can be freed. Selected cells get an accent outline.
- Click a cell to drill in, with the breadcrumb above the map. Backspace or a click on the breadcrumb goes up. Show at most 150 cells per level, merging the smallest into an "Other small items" cell.
- **State pill** at top centre (Proton's "CONNECTED" tab position): "Not scanned", "Scanning · 38%", "Ready · 38.6 GB can be freed", "Cleaning…", "Freed 38.6 GB".
- **Zoom control** top right (Proton's −/+ slider) scales the map. The app name and logo also sit top right.
- **Hover card**, bottom right: name, size, share of disk, last used, risk.
- Before any scan, the canvas shows an empty-state illustration of the volume outline with one action: "Scan this Mac".

### 5.3 Compact layout

Identical sidebar, no canvas. The status card and list are the whole app. The detail drawer becomes a sheet that slides over the sidebar. Every feature except the disk map is available in compact mode.

### 5.4 Other surfaces

- **Clean confirmation sheet**: total size, number of items, grouped by method ("Moved to Trash: 3 items, 2.1 GB · Deleted permanently: 11 items, 36.5 GB · Cleared by tool: Docker, Homebrew"), warnings for any `review`/`caution` items, and a "Close apps first" list if a related app is running (for example, Xcode is open while Xcode build data is selected). Buttons: "Clean 38.6 GB" and "Cancel".
- **Result view**: the freed total counts up, the capacity bar animates to the new free space, and there's a line with the Trash reminder where relevant ("2.1 GB is in the Trash. Empty the Trash to free it."), plus "Empty Trash" and "Done".
- **History** (in Settings): past cleanups with date, total freed, and the per-item deletion log.

### 5.5 States to design for

Every screen needs a designed version of: first launch, not scanned, quick scan running, full scan running, results, nothing to clean ("Your Mac is tidy. 142.3 GB free."), cleaning, partial failure ("Cleaned 36.2 GB. 2 items couldn't be removed" with reasons and a retry), permission missing, disk nearly full (banner), and cancelled scan (keep partial results with a note that they're partial).

### 5.6 Copy and the two label sets

Every rule has an `everyday` label and a `developer` label (§6). Examples:

| Developer label | Everyday label |
|---|---|
| `~/Library/Caches` (app caches) | Leftover files from apps |
| Xcode DerivedData | Temporary files from Xcode |
| npm cache | Downloaded packages (npm) |
| Time Machine local snapshots | Backup snapshots stored on this Mac |
| `~/Library/Logs` | App activity logs |

Copy rules: sentence case, plain verbs, never alarmist. Errors say what happened and what to do. Don't use "junk" to describe people's files. Numbers always show units, with one decimal place above 1 GB.

### 5.7 Design tokens

The desktop app is dark only (matching the reference). There is no light theme and no theme setting, and the window chrome is forced to dark regardless of the OS appearance.

| Token | Value | Use |
|---|---|---|
| `--bg` | `#16141F` | Window background, canvas |
| `--surface` | `#1F1C2C` | Sidebar, cards |
| `--raised` | `#2A2640` | Buttons, rows on hover, inputs |
| `--line` | `#383252` | Dividers, outlines |
| `--text` | `#ECE9F7` | Primary text |
| `--text-muted` | `#9D97B5` | Secondary text |
| `--accent` | `#8B6CFF` | Primary actions, selection, focus |
| `--safe` | `#4CCB9F` | Safe to clean |
| `--review` | `#EDB548` | Needs review |
| `--caution` | `#EE6B7E` | Caution |

Category colours for the map and capacity bar should be six desaturated hues that sit comfortably next to the accent (define them in `tokens.css`, check contrast against `--bg`, `--surface` and `--raised`).

- **Type:** Instrument Sans (SIL OFL, bundled locally, no network fonts) for all UI, with tabular figures (`font-variant-numeric: tabular-nums`) wherever sizes appear. JetBrains Mono (OFL) only for file paths. Scale: 12 / 13 / 15 / 18 / 24 / 32 px. Body text is 13 px, and the free-space figure in the status card is 24 px semibold.
- **Shape:** radius 12 px for cards and the drawer, 8 px for buttons and inputs, 6 px for rows and chips, 2 px for treemap cells. Radius varies by hierarchy on purpose.
- **Depth:** separate layers with surface colour steps and 1 px `--line` borders. Use a shadow only for floating elements (drawer, sheets, hover card).
- **Spacing:** 4 px base grid.

### 5.8 Motion

Motion should explain what changed. It isn't decoration. Use Motion (`motion/react`), with spring transitions for spatial changes (stiffness 380, damping 34) and 150–200 ms ease-out for fades. Everything respects `prefers-reduced-motion`, falling back to instant or opacity-only changes.

Signature moments (build these carefully; they're what people will remember):

1. **Scan sweep.** During a scan, a soft band of accent light sweeps across the map, and cells fade in and grow to their size as results stream in. The capacity bar segments fill left to right.
2. **Clean collapse.** When cleaning finishes, cleaned cells shrink to nothing and their siblings re-flow to fill the space (animate treemap layout between old and new states with shared layout IDs), while the free-space figure counts up.
3. **Expand / collapse.** The canvas slides and fades in or out as the window width animates. The sidebar stays perfectly still.
4. **Row to map link.** On row hover, the connector line draws from row to cell (200 ms), and the cell brightens.

Supporting motion: tab underline slides between tabs; list sections expand with height animation; checkbox ticks draw in; the drawer slides in on a spring; number changes use a rolling-digit counter; skeleton rows while the first results arrive.

Performance rule: animate only `transform` and `opacity`. Hold 60 fps on the treemap with 150 cells, and measure it.

### 5.9 Onboarding (first launch only)

Three short screens, each skippable, in the compact-width layout centred in the window:

1. **Welcome**: "See what's filling your Mac, and clear it safely." Button: "Get started".
2. **Who's using JClean?** Two large cards: "I write code" (Developer mode: "Includes caches from tools like npm, Xcode, Docker and Gradle") and "I just want space back" (Everyday mode: "Simple categories, safe choices"). A note under them: "You can change this anytime in Settings."
3. **Full Disk Access** (macOS): why it's needed, what JClean does and doesn't do with it ("JClean only reads file sizes and dates, never file contents. Nothing leaves your Mac."), an "Open System Settings" button that deep-links to the Full Disk Access pane, and "Skip for now". The screen detects when access is granted and moves on automatically.

After onboarding, a quick scan starts automatically and the main window fills in live.

### 5.10 Menu bar / tray

A small monochrome menu bar icon (optional, on by default) with a menu showing: free space, reclaimable total from the last scan, "Quick scan", "Open JClean", and "Quit". Optionally show free space as text next to the icon (off by default).

### 5.11 Settings

Sections: General (mode, launch at login, menu bar icon, scan on launch), Scanning (project folders to scan, excluded folders, inactive-project threshold, include external drives), Cleaning (default method for user files: Trash or delete; confirm before cleaning, always on for `caution`), Rules (toggle built-in rules, add custom folders or import a rule pack), History, Updates (check automatically, current version), About (licence, GitHub link, privacy statement).

### 5.12 Accessibility

Full keyboard navigation (the list uses arrow keys, Space to toggle, Enter to open details; the map is reachable by keyboard with arrow keys between cells), visible focus rings in `--accent`, VoiceOver labels on every control and map cell ("npm cache, 6.1 gigabytes, safe to clean"), WCAG AA contrast for text, and risk never conveyed by colour alone (always an icon and label too).

## 6. Rules engine

### 6.1 Rule file format

Each file in `rules/<os>/` holds an array of rules for one ecosystem (`node.json`, `xcode.json`, `browsers.json`…). Every file is validated against `rules/schema/rule.schema.json` at build time (a test fails on invalid rules) and at load time for user rules.

```json
{
  "id": "macos.node.npm-cache",
  "version": 1,
  "platforms": ["macos"],
  "audience": ["developer"],
  "category": "developer",
  "group": "Package caches",
  "labels": {
    "developer": "npm cache",
    "everyday": "Downloaded packages (npm)"
  },
  "description": {
    "what": "Copies of every package npm has downloaded, kept so installs are faster.",
    "ifCleared": "npm downloads packages again the next time you install them. Nothing in your projects changes."
  },
  "icon": "package",
  "risk": "safe",
  "regenerates": true,
  "detect": {
    "kind": "fixed",
    "paths": ["{home}/.npm/_cacache"]
  },
  "unused": { "basis": "newest-mtime", "minAgeDays": 0 },
  "cleanup": {
    "method": "command",
    "command": { "tool": "npm", "args": ["cache", "clean", "--force"] },
    "fallback": "delete"
  },
  "relatedApps": [],
  "mayShareBlocks": false,
  "docs": "https://docs.npmjs.com/cli/commands/npm-cache"
}
```

### 6.2 Fields

| Field | Meaning |
|---|---|
| `id` | Stable unique ID, `<os>.<ecosystem>.<name>` |
| `audience` | `everyday`, `developer`, or both. Everyday mode shows rules with `everyday`; Developer mode shows all. |
| `category` | Colour/category bucket: `apps`, `developer`, `system`, `media`, `documents`, `other` |
| `risk` | `safe` (regenerable, no user data), `review` (probably fine, user should look), `caution` (may contain data that can't be recovered), `info` (shown for understanding, never cleanable) |
| `regenerates` | Whether the tool recreates it automatically |
| `detect.kind` | `fixed` (known paths, globs allowed), `project-artifact` (uses §4.5 markers), `probe` (ask a tool), `query` (file pattern search, e.g. old installers in Downloads with `olderThanDays`) |
| `unused` | How "last used" is computed and the minimum age before the item is pre-selectable |
| `cleanup.method` | `delete` (permanent), `trash`, `command`, or `none` |
| `cleanup.command` | Tool name plus argument list. Only built-in rules may use `command` (see §7.4). |
| `cleanup.requiresAdmin` | Needs administrator approval |
| `relatedApps` | Bundle IDs / process names that should be closed first (e.g. `com.apple.dt.Xcode`) |
| `keep` | Optional keep-policy, e.g. `{"newest": 1}` to keep the newest version of each item (IDE versions, simulator runtimes, extension versions). `groupBy`: `parent` (all items in one folder) or `name` (name with its version removed, e.g. `PyCharm2024.1`) |
| `detect.eachChild` / `detect.exclude` | For `fixed`: every entry inside the matched folder is its own item and the folder itself is kept (app caches, DerivedData, Trash). `exclude` lists child-name globs to leave out |
| `detect.names` / `olderThanDays` / `minBytes` / `recursive` | For `query`: file-name globs, age and size limits. `recursive` queries (large files) only run in a full scan |
| `cleanup.keepRoot` | Clear the folder's contents but keep the folder |
| `cleanup.fallback` | Method to use when a `command` tool isn't installed (`delete` or `trash`) |
| `crossFilesystems` | Opt in to measuring and cleaning across a mount point (off by default) |

Path tokens: `{home}`, `{caches}` (macOS `~/Library/Caches`, Windows `%LOCALAPPDATA%`), `{appSupport}`, `{logs}`, `{temp}`, `{localAppData}`, `{appData}`, `{env:NAME}`. Globs use `globset` syntax.

### 6.3 Matching and overlaps

Paths can be claimed by more than one rule (for example, `~/Library/Caches/Yarn` falls under both the Yarn rule and the general app-caches rule). The most specific rule (longest matching root) wins, and the general rule excludes it from its total. Sizes are never counted twice anywhere in the UI.

### 6.4 Custom rules

Users can add a custom folder from Settings → Rules (pick folder, name it, choose risk; the method is always `trash`) or import a rule pack JSON. Custom and imported rules can never use `command` or `delete`, can't target protected paths, and show a "Custom" badge.

## 7. Safety model

### 7.1 SafetyGuard

Every path in a clean plan must pass all of these checks immediately before deletion, not just at scan time:

1. Canonicalize (resolve `..` and any symlinks in parent components), then confirm the canonical path is inside a root declared by the matched rule.
2. Confirm it isn't a protected path or an ancestor of one.
3. Confirm it isn't itself a symlink (a symlink is removed as a link, never followed).
4. Re-stat and compare allocated size and mtime with the scan snapshot. If the size changed by more than 10% or the mtime is newer than the scan, drop it from the plan and report "changed since scan".
5. Confirm no process listed in `relatedApps` is running. If one is, pause and ask the user to close it or skip the item.

### 7.2 Protected paths (never deleted, even if a rule matches)

`/`, `/System`, `/usr` (except `/usr/local/Homebrew` cache via `brew`), `/bin`, `/sbin`, `/etc`, `/private/var/db`, `/Applications` (except explicitly listed installer apps, v2), `/Library` itself, the home directory itself, and the top level of `~/Documents`, `~/Desktop`, `~/Pictures`, `~/Music`, `~/Movies`, `~/Downloads`, `~/Library`, plus `~/.ssh`, `~/.gnupg`, `~/.aws`, `~/.config` (root), `~/.kube`, `~/Library/Keychains`, `~/Library/Mail` (except Mail Downloads), any `.git` directory, and any path containing a file named `.jclean-keep`. Windows equivalents: `C:\`, `C:\Windows` (except the specific cleanup targets in §8.3), `C:\Program Files*`, the user profile root and its top-level known folders.

The guard is enforced in `jclean-core::safety`, has its own exhaustive test suite, and can't be disabled from settings.

### 7.3 Deletion methods

- **Delete** (permanent) is the default only for `safe` + `regenerates: true` items. Moving a cache to the Trash frees no space until the Trash is emptied, which defeats the purpose and confuses users.
- **Trash** is the default for anything that could be user data (old downloads, large files, custom folders, `review` items without `regenerates`). The result screen reminds users to empty the Trash.
- **Command** runs the tool's own cleanup, preferred when the tool manages the data (Docker, Homebrew, simulators, Go module cache whose files are read-only).
- `caution` items always require an explicit second confirmation with the consequence spelled out.

### 7.4 Running external commands

- Commands come only from built-in rules, compiled into the binary.
- The executable is resolved from a fixed list of known locations (`/opt/homebrew/bin`, `/usr/local/bin`, `/usr/bin`, `~/.nvm/versions/node/*/bin`, `~/.cargo/bin`, `~/go/bin`…), since GUI apps on macOS don't inherit the shell `PATH`. If it isn't found, use the rule's `fallback` or mark the item "Tool not found".
- Spawn with an argument vector, never through a shell. Enforce a timeout (default 120 s), capture stdout/stderr for the log, and treat a non-zero exit as a failure with the message shown.
- Admin operations (macOS) go through a single `platform::macos::run_privileged(cmd, args)` that builds a strictly escaped `do shell script … with administrator privileges` call for allowlisted commands only. Keep admin operations to a minimum in v1.

### 7.5 Deletion log

Every executed action is written to SQLite: timestamp, rule ID, path, method, bytes, outcome, error. Visible in Settings → History, and exportable as CSV.

## 8. Detection catalog

Risk: S = safe, R = review, C = caution, I = info only. Method: D = delete, T = trash, Cmd = tool command. Audience: E = everyday, Dev = developer.

### 8.1 macOS — everyday (shown in both modes)

| Item | Location / detection | Risk | Method |
|---|---|---|---|
| App caches (per app breakdown) | `~/Library/Caches/*` minus paths claimed by specific rules | S | D |
| Browser caches: Safari, Chrome, Edge, Brave, Arc, Firefox, Opera | Each browser's cache folders under `~/Library/Caches` and profile `Cache`/`Code Cache` dirs (Safari needs Full Disk Access). Never cookies, history, passwords or profiles. | S | D |
| Chat and media app caches: Slack, Discord, Teams, Zoom, WhatsApp, Spotify (streaming cache only, never offline downloads) | Their `Cache`, `Code Cache`, `GPUCache`, `Service Worker/CacheStorage` dirs | S | D |
| App activity logs | `~/Library/Logs` | S | D |
| Crash reports | `~/Library/Logs/DiagnosticReports` | S | D |
| System logs | `/Library/Logs`, `/private/var/log` (admin) | R | D (admin) |
| Trash | `~/.Trash` | S | Empty |
| Old installers | `~/Downloads/*.{dmg,pkg,iso,xip,zip}` older than 30 days | R | T |
| Large files | Files over 1 GB (setting) in scan roots, unmodified for 90+ days, not matched by another rule | R | T |
| iPhone and iPad backups | `~/Library/Application Support/MobileSync/Backup/*`, labelled with device name and backup date | C | T |
| Mail attachment downloads | `~/Library/Containers/com.apple.mail/Data/Library/Mail Downloads` | S | D |
| Local backup snapshots | `tmutil listlocalsnapshots /` | R | Cmd (`tmutil deletelocalsnapshots <date>`, admin) |
| macOS installer apps | `/Applications/Install macOS *.app` | R | T |
| Photos, Music, Movies libraries | Size only | I | none |
| Purgeable space | From volume query | I | none |

### 8.2 macOS — developer

| Item | Location / detection | Risk | Method |
|---|---|---|---|
| npm cache | `~/.npm/_cacache` | S | Cmd `npm cache clean --force`, fallback D |
| Yarn cache | `~/Library/Caches/Yarn`, `~/.yarn/berry/cache` | S | D |
| pnpm store | `~/Library/pnpm/store` | S | Cmd `pnpm store prune` |
| Bun cache | `~/.bun/install/cache` | S | D |
| Old Node versions | `~/.nvm/versions/node/*`, `~/.volta/tools/image/node/*`, fnm dirs; keep the default/active version | R | D |
| pip / uv / Poetry caches | `~/Library/Caches/pip`, `~/.cache/uv`, `~/Library/Caches/pypoetry` | S | Cmd where available, else D |
| Conda packages | `pkgs/` in `~/miniconda3`, `~/anaconda3`, `~/miniforge3`, `/opt/homebrew/Caskroom/miniconda/base` | S | Cmd `conda clean --all -y` |
| Hugging Face models | `~/.cache/huggingface/hub` | R | D |
| Local LLM models (Ollama, LM Studio) | `~/.ollama/models`, `~/.lmstudio/models`, `~/.cache/lm-studio/models` | R | D |
| Gradle | `~/.gradle/caches`, old versions in `~/.gradle/wrapper/dists` (keep newest), `~/.gradle/daemon` logs | S | D |
| Maven repository | `~/.m2/repository` | R | D |
| Cargo registry and git checkouts | `~/.cargo/registry/{cache,src}`, `~/.cargo/git/checkouts` | S | D |
| Go caches | `~/go/pkg/mod`, `~/Library/Caches/go-build` | S | Cmd `go clean -modcache` / `go clean -cache` |
| Xcode build data (DerivedData) | `~/Library/Developer/Xcode/DerivedData` (per project) | S | D, close Xcode |
| Xcode previews | `~/Library/Developer/Xcode/UserData/Previews` | S | D |
| Xcode archives | `~/Library/Developer/Xcode/Archives` (per archive, with date and app name) | C | T |
| Device support files | `~/Library/Developer/Xcode/{iOS,watchOS,tvOS,visionOS} DeviceSupport/*`, keep newest per platform | R | D |
| Unavailable simulators | `xcrun simctl list devices unavailable` | S | Cmd `xcrun simctl delete unavailable` |
| Simulator devices | `~/Library/Developer/CoreSimulator/Devices/*`, labelled by device name and OS | R | Cmd `xcrun simctl delete <udid>` |
| Simulator runtimes | `xcrun simctl runtime list` | R | Cmd `xcrun simctl runtime delete <id>` |
| Simulator caches | `~/Library/Developer/CoreSimulator/Caches` | S | D |
| CocoaPods / Carthage / SwiftPM caches | `~/Library/Caches/CocoaPods`, `~/Library/Caches/org.carthage.CarthageKit`, `~/Library/Caches/org.swift.swiftpm` | S | D |
| Android emulators | `~/.android/avd/*` | R | D |
| Android system images | `~/Library/Android/sdk/system-images/*` | R | D |
| Android caches | `~/.android/cache`, old Android Studio versions' caches/logs in `~/Library/{Caches,Logs}/Google/AndroidStudio*` | S | D |
| Flutter/Dart pub cache | `~/.pub-cache` | R | Cmd `dart pub cache clean` |
| Docker build cache and dangling images | `docker system df -v` (daemon must be running) | S | Cmd `docker builder prune -f`, `docker image prune -f` |
| Unused Docker images | same | R | Cmd `docker image prune -a -f` |
| Unused Docker volumes | same | C | Cmd `docker volume prune -f` |
| Docker disk image | `~/Library/Containers/com.docker.docker/Data/vms/0/data/Docker.raw` | I | none (explain that pruning frees space inside it and Docker Desktop reclaims it) |
| Homebrew | `~/Library/Caches/Homebrew`, old formula versions | S | Cmd `brew cleanup -s` |
| JetBrains caches and logs for uninstalled versions | `~/Library/Caches/JetBrains/<Product><ver>`, `~/Library/Logs/JetBrains/*` where that version isn't installed | S | D |
| JetBrains settings for uninstalled versions | `~/Library/Application Support/JetBrains/<Product><ver>` | R | T |
| VS Code, Cursor, Windsurf caches | `Cache`, `CachedData`, `CachedExtensionVSIXs`, `Code Cache`, `logs` under each editor's Application Support folder | S | D |
| Old editor extension versions | `~/.vscode/extensions`, `~/.cursor/extensions`: older versions when a newer version of the same extension exists | S | D |
| Browser automation binaries | `~/Library/Caches/ms-playwright`, `~/.cache/puppeteer` | R | D |
| Electron and other tool caches | `~/Library/Caches/electron`, `~/Library/Caches/composer`, `~/Library/Caches/node-gyp` | S | D |
| Project build folders | §4.5, grouped by project | S / R | D |

### 8.3 Windows (phase 8 preview)

`%TEMP%` and `%LOCALAPPDATA%\Temp` (S, D, skip files in use); `%LOCALAPPDATA%\npm-cache`, `%LOCALAPPDATA%\Yarn\Cache`, `%LOCALAPPDATA%\pip\Cache`, `%USERPROFILE%\.gradle\caches`, `%USERPROFILE%\.m2`, `%USERPROFILE%\.cargo\registry`, `%LOCALAPPDATA%\go-build`; NuGet (`dotnet nuget locals all --clear`); Visual Studio component caches; browser caches under each profile in `%LOCALAPPDATA%`; `%LOCALAPPDATA%\CrashDumps`; Recycle Bin; Windows Update downloads `C:\Windows\SoftwareDistribution\Download` (R, admin); Delivery Optimization cache (`Delete-DeliveryOptimizationCache`, admin); component store cleanup (`DISM /Online /Cleanup-Image /StartComponentCleanup`, R, admin); `C:\Windows.old` (C, admin, explain that it removes the ability to roll back); `hiberfil.sys`, `pagefile.sys`, WSL and Docker `.vhdx` files (I, with guidance on compacting). Uses `IFileOperation` for Recycle Bin moves and UAC elevation for admin items.

## 9. Cleaning flow

1. The user selects items (safe items with an inactive/unused status are pre-selected; nothing else is).
2. The planner builds a `CleanPlan`: items, methods, total, warnings, related running apps, admin needs.
3. The confirmation sheet (§5.4) shows the plan. `caution` items need a second confirmation.
4. On confirm, the cleaner runs items in parallel where independent (commands run sequentially per tool), running SafetyGuard on each path first, and streams progress per item.
5. Afterwards, the volume's free space is re-read to report actual freed space (not just the estimate), then the result view shows it with the clean-collapse animation, and the plan and outcomes go to history.
6. Failures never stop the rest of the plan. They're listed with the reason and a retry option.

## 10. Local data and privacy

- **Settings**: `settings.json` in the app config directory (`~/Library/Application Support/app.jclean/`), versioned with migrations.
- **History**: `history.sqlite` in the same directory. Tables: `scans` (id, started, finished, mode, totals), `cleanups` (id, scan_id, time, planned_bytes, freed_bytes), `actions` (the deletion log, §7.5). Old scans older than 180 days are pruned; cleanups are kept.
- **Scan cache**: the last full scan's aggregated tree is saved so the map shows instantly on next launch (labelled "From your last scan, 2 days ago" until a fresh scan finishes).
- **Network**: the only network request is the update check against GitHub Releases, which can be turned off. No telemetry, crash reporting, or analytics. State this plainly in About and on the website.
- **Privacy statement** (in-app and on the site): JClean reads file names, sizes and dates to calculate storage. It never reads file contents, never uploads anything, and has no account system.

## 11. Permissions (macOS)

- Full Disk Access is optional but recommended. Without it, Safari caches, Mail downloads, iPhone backups and some app containers can't be measured. Those rows show "Needs Full Disk Access" with a button, rather than disappearing.
- Detect access by attempting `stat` on a protected path (e.g. `~/Library/Safari`). Deep link: `x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles`. Poll every 2 s while the onboarding permission screen or banner is visible.
- The app is not sandboxed (required to see the whole disk), which is why it isn't on the Mac App Store.
- Launch at login via `tauri-plugin-autostart` (SMAppService on macOS 13+).

## 12. Download website

A static marketing and download site in `apps/web`, built with Astro and deployed to Cloudflare Pages. It uses the same design tokens and fonts as the app so it feels like the same product.

### 12.1 Pages

| Route | Content |
|---|---|
| `/` | Hero, how it works, what it finds, safety and privacy, open source, download |
| `/download` | All platforms, version, file size, SHA-256 checksums, install steps (including the Gatekeeper "Open Anyway" steps while builds are unsigned), system requirements |
| `/safety` | How JClean decides what's safe, the risk levels, what it never touches, privacy statement |
| `/rules` | Browsable catalog of everything JClean detects, generated at build time from `rules/*.json`, so it's always accurate |
| `/changelog` | Built from GitHub Releases at build time |

### 12.2 Hero and download button

- The hero's memorable element is a live, in-browser version of the disk map: a sample Mac's storage fills in with the scan sweep, reclaimable cells light up, then collapse as the freed-space figure counts up. It's built with the same treemap component logic as the app (shared package `packages/treemap` in the workspace), not a video. With reduced motion, it shows the final state.
- The download button detects the visitor's OS (`navigator.userAgentData.platform`, falling back to the user agent):
  - macOS: "Download for Mac" (the universal build, so Apple Silicon vs Intel detection isn't needed), with "macOS 12 or later · 12 MB" under it.
  - Windows or Linux (before those builds ship): "Coming to Windows soon", with a link to watch releases on GitHub. No email sign-up, which keeps the site backend-free.
  - Phones and tablets: "JClean is a desktop app. Open this page on your computer to download it." with a copy-link button.
  - Always offer "Other platforms and versions" linking to `/download`.
- Download links point at stable GitHub URLs, `https://github.com/<owner>/jclean/releases/latest/download/JClean_macos_universal.dmg`, so the site never needs editing when a version ships. Version number and size are fetched at build time, and each release triggers a site rebuild through a Cloudflare deploy hook.

### 12.3 Quality bar

Lighthouse 95+ on all four scores, no layout shift, fonts self-hosted, no third-party scripts or cookies, Open Graph image, sitemap, and fully responsive down to 360 px.

## 13. Build phases

Finish each phase's acceptance criteria and pause for review before moving on.

### Phase 0 — Foundations
Monorepo (pnpm + Cargo workspaces), Tauri 2 app with transparent title bar, design tokens and bundled fonts, lint/format/typecheck, GitHub Actions running tests on a macOS runner, tauri-specta type generation.
*Done when:* `pnpm tauri dev` opens an empty branded window in the dark theme, and CI is green.

### Phase 1 — Core engine
`env`, rules loader with schema validation, scanner (quick and full), sizing (§4.2), project detection, probes, SafetyGuard, cleaner with dry-run, history database, `jclean-cli`.
*Done when:* CLI quick scan lists expected items on fixture homes; sizes match `du -k` within 2%; the safety test suite (§16) passes; `jclean-cli clean --dry-run` prints an accurate plan.

### Phase 2 — App shell and design system
Sidebar (status card, tabs, filter row, search, grouped virtualized list with expandable rows), detail drawer, expanded/compact switching with animation, all on mock data.
*Done when:* both window modes look right at all supported sizes, the list handles 5,000 rows smoothly, and everything is keyboard-operable.

### Phase 3 — Live scanning and the disk map
IPC wiring, streamed results, capacity bar, treemap with drill-in and breadcrumb, row-to-cell linking, hover card, scan sweep animation, scan cancellation, last-scan cache.
*Done when:* performance budgets in §15 are met, and results appear progressively.

### Phase 4 — Cleaning
Selection and pre-selection rules, planner, confirmation sheet, execution with progress, related-app checks, result view with clean-collapse animation and actual-freed measurement, history and deletion log.
*Done when:* end-to-end cleaning works on a fixture home, every failure path shows a clear message, and measured freed space matches the estimate within 5% for `delete` items.

### Phase 5 — Onboarding, modes and settings
Onboarding flow, Everyday/Developer modes with both label sets, Full Disk Access flow, all settings sections, custom folders and rule pack import, menu bar icon, launch at login.
*Done when:* a new user can go from first launch to first clean without reading any docs, in either mode.

### Phase 6 — Polish and first release
Motion pass on all four signature moments, reduced-motion pass, accessibility audit (VoiceOver walkthrough, contrast check), all empty/error states from §5.5, auto-updater, release workflow, signing and notarization if the Apple account is ready.
*Done when:* v1.0.0 is published on GitHub Releases with stable asset names and a working update feed.

### Phase 7 — Website
Everything in §12, deployed.
*Done when:* the live site serves the correct download per OS and passes the §12.3 quality bar.

### Phase 8 — Windows
`platform::windows`, Windows rules (§8.3), Recycle Bin, UAC elevation, Mica/Acrylic window backdrop where supported, NSIS installer, Windows build in the release workflow, website button enabled.

### Phase 9 — Linux
`platform::linux`, XDG paths, Linux rules (apt/dnf caches, journal logs, Flatpak/Snap leftovers, the developer set), AppImage and `.deb` builds.

## 14. Distribution and updates

- **Release workflow**: pushing a `v*` tag runs `tauri-action`, builds the macOS universal binary (`--target universal-apple-darwin`), signs and notarizes when the secrets are present, renames assets to stable names (`JClean_macos_universal.dmg`, later `JClean_windows_x64_setup.exe`), generates SHA-256 checksums and the updater's `latest.json`, publishes the GitHub Release, and triggers the website rebuild.
- **Updater**: `tauri-plugin-updater` against `latest.json` on GitHub Releases, with its own signing key pair (free, generated with `tauri signer generate`, private key stored as a GitHub secret). Updates download in the background and apply on the user's confirmation.
- **macOS signing**: without an Apple Developer Program membership ($99/year), builds can't be notarized. On current macOS versions, users then have to approve the app under System Settings → Privacy & Security → "Open Anyway", which hurts trust for a tool that deletes files. Enrol before the public launch. Until then, the download page explains the steps.
- **Windows signing** (phase 8): unsigned installers trigger SmartScreen warnings. Budget for a signing service (e.g. Azure Artifact Signing) before the Windows launch.
- **Later channels**: Homebrew cask, winget.

## 15. Performance budgets

| Metric | Target |
|---|---|
| Installer size (macOS) | < 15 MB |
| Launch to interactive | < 1.5 s |
| Quick scan | < 5 s |
| Full scan, home dir with ~1M files, Apple Silicon SSD | < 45 s |
| Memory, idle | < 120 MB |
| Memory, during full scan | < 400 MB |
| Animation frame rate | 60 fps, measured |

To stay within memory, the full scan keeps aggregated totals per directory and stores individual files only above a size threshold (1 MB by default). The treemap drills in by requesting children from the backend, rather than holding the whole tree in the frontend.

## 16. Testing

- **Core unit tests** for sizing, matching, overlap resolution, unused detection and plan building.
- **Fixture homes**: a test helper builds fake home directories in temp dirs (npm caches, projects with `node_modules`, DerivedData, nested symlinks, hard links, read-only files) with controlled mtimes using the `filetime` crate. No test ever touches the real home.
- **Safety suite** (must be exhaustive): symlink escapes (a symlink inside a cache pointing to `~/Documents`), `..` traversal, protected paths and their ancestors, paths with spaces, Unicode and newlines, files changed after the scan, read-only files, running related apps, and property-based tests (`proptest`) asserting that the guard never approves a path outside the rule roots.
- **Rule tests**: every rule file validates against the schema, has unique IDs, and has both label sets and both description fields.
- **Frontend**: Vitest for stores and components. Playwright runs against the Vite frontend with the Tauri IPC mocked (`@tauri-apps/api/mocks`), since Tauri's WebDriver doesn't support macOS. Include screenshot tests for expanded/compact × key states.
- **Manual release checklist**: a fresh macOS user account with and without Full Disk Access, clean install, update from the previous version, and one real cleanup of each built-in developer category on a test machine.

## 17. Later

Duplicate file finder, app uninstaller with leftover detection, scheduled scans and low-disk alerts, auto-clean rules, external drive scanning, a public CLI release, a community rule-pack repository, and localization.
