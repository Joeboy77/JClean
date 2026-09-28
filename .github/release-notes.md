## Download

- **Mac: [JClean_macos_universal.dmg](https://github.com/Joeboy77/JClean/releases/latest/download/JClean_macos_universal.dmg)**. Apple Silicon and Intel, macOS 12 or later.
- **Windows: [JClean_windows_x64_setup.exe](https://github.com/Joeboy77/JClean/releases/latest/download/JClean_windows_x64_setup.exe)**. Windows 10 or 11, 64-bit. Installs for your account only, no administrator needed.
- **Linux: [JClean_linux_amd64.deb](https://github.com/Joeboy77/JClean/releases/latest/download/JClean_linux_amd64.deb)** for Ubuntu, Debian and Mint, or **[JClean_linux_x86_64.AppImage](https://github.com/Joeboy77/JClean/releases/latest/download/JClean_linux_x86_64.AppImage)** for any other distribution. 64-bit.

Checksums are in `SHA256SUMS.txt`.

## Opening JClean the first time on a Mac

This version isn't notarized by Apple yet, so macOS asks you to confirm once:

1. Open the DMG and drag **JClean** into **Applications**.
2. Open JClean. macOS says it can't verify the app. Click **Done** (not "Move to Trash").
3. Open **System Settings → Privacy & Security**, scroll down to "JClean was blocked to protect your Mac", and click **Open Anyway**. Confirm with your password.
4. JClean opens. From now on it opens normally.

## Installing on Windows

The installer isn't code-signed yet, so Windows SmartScreen may say it "protected your PC". Click **More info**, then **Run anyway**. You only see this when installing; updates install from inside JClean.

If **Smart App Control** blocks it instead (some new Windows 11 PCs), there's no Run anyway button: the installer can only run with Smart App Control turned off, which on most versions of Windows 11 can't be turned back on without reinstalling Windows. A signed installer is planned.

## Installing on Linux

- **.deb:** `sudo apt install ./JClean_linux_amd64.deb`
- **AppImage:** install FUSE 2 first (`sudo apt install libfuse2t64` on Ubuntu 24.04+, `libfuse2` on 22.04 and Debian, `sudo dnf install fuse-libs` on Fedora), then right-click the file → Properties → **Executable as Program** (or `chmod +x JClean_linux_x86_64.AppImage`) and open it.

Both update themselves from Settings → Updates. On GNOME, the tray icon needs the AppIndicator extension (Ubuntu has it already).

JClean is open source, reads only file sizes and dates, and never sends anything off your computer.
