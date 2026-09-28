// Sizes in decimal units (GB = 10⁹ bytes) to match Finder; one decimal from 1 GB up.

const KB = 1e3;
const MB = 1e6;
const GB = 1e9;
const TB = 1e12;

/** One decimal, rounded half up like the engine (`toFixed` rounds 38.65 down). */
function tenths(bytes: number, unit: number): string {
  return (Math.round(bytes / (unit / 10)) / 10).toFixed(1);
}

export function formatBytes(bytes: number): string {
  if (bytes >= TB) return `${tenths(bytes, TB)} TB`;
  if (bytes >= GB) return `${tenths(bytes, GB)} GB`;
  if (bytes >= MB) return `${String(Math.round(bytes / MB))} MB`;
  if (bytes >= KB) return `${String(Math.round(bytes / KB))} KB`;
  return bytes === 1 ? "1 byte" : `${String(Math.round(bytes))} bytes`;
}

/** Spoken form for screen readers: "6.1 gigabytes". */
export function speakBytes(bytes: number): string {
  return formatBytes(bytes)
    .replace(" TB", " terabytes")
    .replace(" GB", " gigabytes")
    .replace(" MB", " megabytes")
    .replace(" KB", " kilobytes");
}

const DAY = 86_400;

/** "today", "3 days ago", "7 months ago", "2 years ago". */
export function formatAgo(unixSecs: number, nowSecs: number = Date.now() / 1000): string {
  const days = Math.max(0, Math.floor((nowSecs - unixSecs) / DAY));
  if (days === 0) return "today";
  if (days === 1) return "yesterday";
  if (days < 60) return `${String(days)} days ago`;
  if (days < 730) return `${String(Math.floor(days / 30))} months ago`;
  return `${String(Math.floor(days / 365))} years ago`;
}

export function basename(path: string): string {
  const trimmed = path.replace(/\/+$/, "");
  return trimmed.slice(trimmed.lastIndexOf("/") + 1);
}

/** Shows the home folder as `~`. */
export function tildify(path: string, home: string): string {
  if (path === home) return "~";
  return path.startsWith(`${home}/`) ? `~${path.slice(home.length)}` : path;
}
