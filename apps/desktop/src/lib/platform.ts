// The words and bits of UI that differ between macOS, Windows and Linux. The
// WebView's user agent says which one we're on; the browser build and tests
// get the Mac wording.

export type Platform = "mac" | "windows" | "linux";

function detect(): Platform {
  if (typeof navigator === "undefined") return "mac";
  const ua = navigator.userAgent;
  if (ua.includes("Windows")) return "windows";
  if (ua.includes("Linux") || ua.includes("X11")) return "linux";
  return "mac";
}

export const platform: Platform = detect();
export const isWindows = platform === "windows";
export const isMac = platform === "mac";

interface Words {
  /** "macOS" / "Windows" / "Linux" */
  os: string;
  /** "Mac" / "PC" / "computer", as in "Scan this Mac". */
  computer: string;
  /** "Trash" / "Recycle Bin" */
  trash: string;
  /** "Finder" / "File Explorer" / "Files" */
  fileManager: string;
  /** Where the tray icon lives: "menu bar" / "system tray". */
  tray: string;
  /** The Settings shortcut. */
  settingsShortcut: string;
  /** Folders always skipped when looking for projects. */
  skippedFolders: string;
  /** Developer tools named in onboarding. */
  devTools: string;
}

const WORDS: Record<Platform, Words> = {
  mac: {
    os: "macOS",
    computer: "Mac",
    trash: "Trash",
    fileManager: "Finder",
    tray: "menu bar",
    settingsShortcut: "⌘,",
    skippedFolders: "Library, the Trash and iCloud folders",
    devTools: "npm, Xcode, Docker and Gradle",
  },
  windows: {
    os: "Windows",
    computer: "PC",
    trash: "Recycle Bin",
    fileManager: "File Explorer",
    tray: "system tray",
    settingsShortcut: "Ctrl+,",
    skippedFolders: "AppData and OneDrive folders",
    devTools: "npm, Visual Studio, Docker and Gradle",
  },
  linux: {
    os: "Linux",
    computer: "computer",
    trash: "Trash",
    fileManager: "Files",
    tray: "system tray",
    settingsShortcut: "Ctrl+,",
    skippedFolders: "~/.cache, ~/.local, Flatpak, Snap and synced folders",
    devTools: "npm, Docker, Gradle and Cargo",
  },
};

export const words: Words = WORDS[platform];

/** Only macOS has Full Disk Access to ask for. */
export const hasFullDiskAccessStep = isMac;

/** macOS draws the traffic lights over the sidebar; elsewhere the window has
 * its own title bar. */
export const hasOverlayTitleBar = isMac;
