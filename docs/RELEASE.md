# Releasing JClean

Pushing a tag like `v1.0.0` runs `.github/workflows/release.yml`. It builds the universal macOS app, signs the update package, and publishes the release. It also signs and notarizes the app when the Apple secrets are set.

## One-time setup: GitHub secrets

Add these in the repository's Settings → Secrets and variables → Actions.

| Secret                                                                                                                     | Required                       | What it is                                                                                                                                                             |
| -------------------------------------------------------------------------------------------------------------------------- | ------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `TAURI_SIGNING_PRIVATE_KEY`                                                                                                | Yes                            | The contents of `~/.tauri/jclean.key` on the machine that generated it. Signs update packages; the matching public key is in `apps/desktop/src-tauri/tauri.conf.json`. |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`                                                                                       | Only if the key has a password | Don't add it for the current key, which has none. GitHub can't store an empty secret, and a missing secret already reads as empty.                                     |
| `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` | No                             | Developer ID signing and notarization. Without them the app is unsigned and users must use "Open Anyway" (spec §14).                                                   |
| `CLOUDFLARE_DEPLOY_HOOK`                                                                                                   | No                             | Rebuilds the website after a release (phase 7).                                                                                                                        |

Keep a backup of `~/.tauri/jclean.key`. If it's lost, existing installs can't be updated and users have to reinstall.

## Cutting a release

1. Set the version in `Cargo.toml` (`[workspace.package] version`) and `apps/desktop/package.json`, then commit.
2. Tag and push: `git tag v1.0.0 && git push origin v1.0.0`.
3. The workflow refuses a tag that doesn't match the version. On success it publishes:
   - `JClean_macos_universal.dmg`, at a stable URL: `https://github.com/Joeboy77/JClean/releases/latest/download/JClean_macos_universal.dmg`
   - `SHA256SUMS.txt`
   - `latest.json` and the signed `.app.tar.gz` that the in-app updater uses

## Manual checklist (spec §16)

Before announcing a release:

- [ ] Fresh macOS user account, **without** Full Disk Access: onboarding, "Needs Full Disk Access" rows, banner, first clean.
- [ ] Same account after granting Full Disk Access: JClean notices on its own and rescans.
- [ ] Clean install from the DMG (including Gatekeeper steps if unsigned).
- [ ] Update from the previous version through Settings → Updates → Restart to update.
- [ ] One real cleanup of each built-in developer category on a test machine.
- [ ] Administrator cleanup: system logs, password prompt, cancel and retry.
- [ ] VoiceOver walkthrough: sidebar, list (arrow keys, Space, Enter), map (arrow keys, Enter, Backspace), confirmation sheet, Settings.
- [ ] Reduced motion (System Settings → Accessibility → Display → Reduce motion): no sweeps, shimmers or sliding; everything still works.
- [ ] Compact window (below 760 px wide) for every screen.

## Measured budgets (spec §15)

Measured on an Apple Silicon Mac with a 1.9-million-file home folder:

| Metric                  | Target   | Measured                                                                                                                         |
| ----------------------- | -------- | -------------------------------------------------------------------------------------------------------------------------------- |
| Installer size          | < 15 MB  | 4.3 MB (Apple Silicon build)                                                                                                     |
| Launch to window        | < 1.5 s  | 0.91 s                                                                                                                           |
| Quick scan              | < 5 s    | 3.7–4.3 s with the folder-size cache (17.6 s the very first time)                                                                |
| Full scan               | < 45 s   | 44.5 s (273 GB, 490k folders)                                                                                                    |
| Memory, idle            | < 120 MB | ~95 MB: 35 MB app process + 60 MB page renderer (WebKit's GPU and network helpers add ~57 MB resident, partly shared with macOS) |
| Memory during full scan | < 400 MB | 328 MB                                                                                                                           |
| Map updates             | 60 fps   | 8–20 ms per update in the development build                                                                                      |
