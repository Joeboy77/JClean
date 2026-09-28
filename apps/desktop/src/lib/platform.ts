// The words and bits of UI that differ between macOS and Windows. The
// WebView's user agent says which one we're on; the browser build and tests
// get the Mac wording.

export const isWindows =
  typeof navigator !== "undefined" && navigator.userAgent.includes("Windows");

interface Words {
  /** "macOS" / "Windows" */
  os: string;
  /** "Mac" / "PC", as in "Scan this Mac". */
  computer: string;
  /** "Trash" / "Recycle Bin" */
  trash: string;
  /** "Finder" / "File Explorer" */
  fileManager: string;
  /** Where the tray icon lives: "menu bar" / "system tray". */
  tray: string;
  /** The Settings shortcut. */
  settingsShortcut: string;
  /** Folders always skipped when looking for projects. */
  skippedFolders: string;
}

export const words: Words = isWindows
  ? {
      os: "Windows",
      computer: "PC",
      trash: "Recycle Bin",
      fileManager: "File Explorer",
      tray: "system tray",
      settingsShortcut: "Ctrl+,",
      skippedFolders: "AppData and OneDrive folders",
    }
  : {
      os: "macOS",
      computer: "Mac",
      trash: "Trash",
      fileManager: "Finder",
      tray: "menu bar",
      settingsShortcut: "⌘,",
      skippedFolders: "Library, the Trash and iCloud folders",
    };

/** Windows has no Full Disk Access to ask for. */
export const hasFullDiskAccessStep = !isWindows;
