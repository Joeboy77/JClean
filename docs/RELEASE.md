# Releasing JClean

Pushing a tag like `v1.0.0` runs `.github/workflows/release.yml`. It builds the universal macOS app, the Windows x64 installer and the Linux AppImage and .deb, signs their update packages, and publishes the release. It also signs and notarizes the app when the Apple secrets are set.

## One-time setup: GitHub secrets

Add these in the repository's Settings → Secrets and variables → Actions.

| Secret                                                                                                                     | Required                       | What it is                                                                                                                                                             |
| -------------------------------------------------------------------------------------------------------------------------- | ------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `TAURI_SIGNING_PRIVATE_KEY`                                                                                                | Yes                            | The contents of `~/.tauri/jclean.key` on the machine that generated it. Signs update packages; the matching public key is in `apps/desktop/src-tauri/tauri.conf.json`. |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`                                                                                       | Only if the key has a password | Don't add it for the current key, which has none. GitHub can't store an empty secret, and a missing secret already reads as empty.                                     |
| `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` | No                             | Developer ID signing and notarization. Without them the app is unsigned and users must use "Open Anyway" (spec §14).                                                   |
| `CLOUDFLARE_DEPLOY_HOOK`                                                                                                   | No                             | Rebuilds the website after a release (phase 7).                                                                                                                        |

Keep a backup of `~/.tauri/jclean.key`. If it's lost, existing installs can't be updated and users have to reinstall.

## One-time setup: the website on Cloudflare Pages

The download site in `apps/web` is static. It reads the latest release (version, DMG size, checksums, notes) from GitHub when it builds, and the download button always points at `/releases/latest/download/JClean_macos_universal.dmg`, so a new release only needs a rebuild.

1. In Cloudflare, go to **Workers & Pages → Create → Pages → Connect to Git** and pick `Joeboy77/JClean`.
2. Build settings:
   - Framework preset: **Astro**
   - Build command: `pnpm install --frozen-lockfile && pnpm --filter web build`
   - Build output directory: `apps/web/dist`
   - Root directory: leave empty (the repository root, so the workspace packages and `rules/` resolve)
3. Environment variables (Production and Preview):
   - `NODE_VERSION` = `22`
   - `PNPM_VERSION` = the version in the root `package.json` `packageManager` field
   - `SITE_URL` = the site's address, e.g. `https://jclean.pages.dev` or your own domain. Used for canonical links, the sitemap and `robots.txt`.
   - `GITHUB_TOKEN` (optional) = a fine-grained token with no permissions. Raises the GitHub API limit; the build falls back safely without it.
4. **Settings → Builds → Deploy hooks**: create a hook for `main`, and save its URL as the `CLOUDFLARE_DEPLOY_HOOK` GitHub secret. The release workflow calls it after publishing, so the site shows the new version.

## Cutting a release

1. Set the version in `Cargo.toml` (`[workspace.package] version`) and `apps/desktop/package.json`, then commit.
2. Tag and push: `git tag v1.0.0 && git push origin v1.0.0`.
3. The workflow refuses a tag that doesn't match the version. On success it publishes:
   - `JClean_macos_universal.dmg`, at a stable URL: `https://github.com/Joeboy77/JClean/releases/latest/download/JClean_macos_universal.dmg`
   - `JClean_windows_x64_setup.exe`, at `https://github.com/Joeboy77/JClean/releases/latest/download/JClean_windows_x64_setup.exe`
   - `JClean_linux_x86_64.AppImage` and `JClean_linux_amd64.deb`, at the same kind of URL
   - `SHA256SUMS.txt` for all of them
   - `latest.json` and the signed update packages the in-app updater uses (the `.app.tar.gz`, the `-setup.exe`, the AppImage and the `.deb`)

The website offers the Windows and Linux downloads as soon as a release includes them; until then those visitors see "Coming soon".

## Windows signing

The Windows installer isn't code-signed, so SmartScreen shows "Windows protected your PC" until the download builds reputation. The download page and release notes explain **More info → Run anyway**. Before a wide Windows launch, budget for a signing service such as Azure Artifact Signing (spec §14) and add its secrets to the Windows job.

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

On Windows (10 and 11):

- [ ] Install from `JClean_windows_x64_setup.exe` through SmartScreen's Run anyway, without administrator rights.
- [ ] Windows 11: Mica shows behind the sidebar. Windows 10: the window is solid, never see-through.
- [ ] Quick scan finds Temp, the Recycle Bin, browser caches and developer caches; nothing in OneDrive is downloaded.
- [ ] Clean something in use (a file open in another app in %TEMP%): it's reported as in use, the rest is cleaned.
- [ ] Administrator cleanup (Windows Update downloads): one UAC prompt; declining it skips those items.
- [ ] Empty Recycle Bin from the result screen; Explorer shows it empty.
- [ ] Tray icon is visible on a dark taskbar, with free space in its tooltip.
- [ ] Update from the previous version through Settings → Updates.

On Linux (Ubuntu with GNOME, and one KDE or Fedora machine):

- [ ] AppImage: runs after `chmod +x` (with `libfuse2t64` on Ubuntu 24.04); .deb: `sudo apt install ./JClean_linux_amd64.deb` pulls in what it needs.
- [ ] Quick scan finds ~/.cache, the Trash, Flatpak/Snap app caches and developer caches; `XDG_CACHE_HOME` pointing elsewhere is followed.
- [ ] Administrator cleanup (APT or DNF cache, journal, an old snap revision): one polkit prompt; dismissing it skips those items.
- [ ] A removed Flatpak app's data in ~/.var/app is listed; an installed app's isn't.
- [ ] Tray icon shows in the top bar (GNOME needs the AppIndicator extension).
- [ ] Update from the previous version, both as an AppImage and as a .deb.

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
