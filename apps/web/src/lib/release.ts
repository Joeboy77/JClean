// Release facts read at build time (spec §12.2). Each release triggers a
// rebuild through the Cloudflare deploy hook, so the site never goes stale.
// If GitHub can't be reached, the site still builds with safe fallbacks.

export const REPO = "Joeboy77/JClean";
export const DMG = "JClean_macos_universal.dmg";
/** Stable URL: always the newest release's DMG. */
export const DMG_URL = `https://github.com/${REPO}/releases/latest/download/${DMG}`;
export const EXE = "JClean_windows_x64_setup.exe";
/** Stable URL: always the newest release's Windows installer. */
export const EXE_URL = `https://github.com/${REPO}/releases/latest/download/${EXE}`;
export const APPIMAGE = "JClean_linux_x86_64.AppImage";
export const APPIMAGE_URL = `https://github.com/${REPO}/releases/latest/download/${APPIMAGE}`;
export const DEB = "JClean_linux_amd64.deb";
export const DEB_URL = `https://github.com/${REPO}/releases/latest/download/${DEB}`;
export const RELEASES_URL = `https://github.com/${REPO}/releases`;

export interface Checksum {
  sha256: string;
  file: string;
}

export interface LatestRelease {
  version: string | null;
  /** Bytes. */
  dmgSize: number | null;
  /** Bytes; `null` when the latest release has no Windows installer yet. */
  exeSize: number | null;
  /** Bytes; `null` when the latest release has no Linux AppImage yet. */
  appImageSize: number | null;
  publishedAt: string | null;
  checksums: Checksum[];
}

export interface ReleaseNote {
  version: string;
  publishedAt: string;
  url: string;
  /** Rendered and sanitized by GitHub. */
  html: string;
}

function headers(accept = "application/vnd.github+json"): Record<string, string> {
  const token = process.env.GITHUB_TOKEN;
  return { Accept: accept, ...(token ? { Authorization: `Bearer ${token}` } : {}) };
}

async function json(url: string, accept?: string): Promise<unknown> {
  try {
    const res = await fetch(url, { headers: headers(accept) });
    return res.ok ? await res.json() : null;
  } catch {
    return null;
  }
}

function isRecord(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null;
}

/** The newest tag, from where /releases/latest redirects. No API, no rate limit. */
async function latestTag(): Promise<string | null> {
  try {
    const res = await fetch(`https://github.com/${REPO}/releases/latest`, { redirect: "manual" });
    const location = res.headers.get("location") ?? "";
    const tag = /\/releases\/tag\/(v?[^/?#]+)/.exec(location)?.[1];
    return tag ?? null;
  } catch {
    return null;
  }
}

/** A download's size from its headers; `null` if it doesn't exist. */
async function downloadSize(url: string): Promise<number | null> {
  try {
    const res = await fetch(url, { method: "HEAD", redirect: "follow" });
    const length = Number(res.headers.get("content-length"));
    return res.ok && length > 0 ? length : null;
  } catch {
    return null;
  }
}

let latest: Promise<LatestRelease> | undefined;

export function latestRelease(): Promise<LatestRelease> {
  latest ??= (async () => {
    const data = await json(`https://api.github.com/repos/${REPO}/releases/latest`);
    const assets = isRecord(data) && Array.isArray(data.assets) ? data.assets.filter(isRecord) : [];
    const size = async (name: string, url: string) => {
      const asset = assets.find((a) => a.name === name);
      return asset && typeof asset.size === "number" ? asset.size : downloadSize(url);
    };
    let checksums: Checksum[] = [];
    try {
      const res = await fetch(`https://github.com/${REPO}/releases/latest/download/SHA256SUMS.txt`);
      if (res.ok) {
        checksums = (await res.text())
          .split("\n")
          .map((line) => line.trim().split(/\s+/))
          .filter(
            (parts): parts is [string, string] =>
              parts.length === 2 && /^[0-9a-f]{64}$/.test(parts[0] ?? ""),
          )
          .map(([sha256, file]) => ({ sha256, file }));
      }
    } catch {
      checksums = [];
    }
    const tag =
      isRecord(data) && typeof data.tag_name === "string" ? data.tag_name : await latestTag();
    return {
      version: tag ? tag.replace(/^v/, "") : null,
      dmgSize: await size(DMG, DMG_URL),
      exeSize: await size(EXE, EXE_URL),
      appImageSize: await size(APPIMAGE, APPIMAGE_URL),
      publishedAt:
        isRecord(data) && typeof data.published_at === "string" ? data.published_at : null,
      checksums,
    };
  })();
  return latest;
}

function text(value: unknown): string {
  return typeof value === "string" ? value : "";
}

/** Published releases, newest first, for /changelog. */
export async function releaseNotes(): Promise<ReleaseNote[]> {
  const data = await json(
    `https://api.github.com/repos/${REPO}/releases?per_page=30`,
    "application/vnd.github.html+json",
  );
  if (!Array.isArray(data)) return [];
  return data
    .filter(isRecord)
    .filter((r) => r.draft !== true)
    .map((r) => ({
      version: text(r.tag_name).replace(/^v/, ""),
      publishedAt: text(r.published_at),
      url: text(r.html_url) || RELEASES_URL,
      html: text(r.body_html),
    }));
}

export interface Download {
  url: string;
  /** e.g. "macOS 12 or later · 8.9 MB" */
  detail: string;
  /** A second format, e.g. the .deb next to the AppImage. */
  alternative?: { url: string; label: string };
}

/** What the download buttons offer. Windows appears once a release has it. */
export function downloads(r: LatestRelease): {
  mac: Download;
  windows: Download | null;
  linux: Download | null;
} {
  const detail = (base: string, size: number | null) =>
    [base, size ? formatSize(size) : null].filter(Boolean).join(" · ");
  return {
    mac: { url: DMG_URL, detail: detail("macOS 12 or later", r.dmgSize) },
    windows:
      r.exeSize === null
        ? null
        : { url: EXE_URL, detail: detail("Windows 10 or 11, 64-bit", r.exeSize) },
    linux:
      r.appImageSize === null
        ? null
        : {
            url: APPIMAGE_URL,
            detail: detail("AppImage, any distribution, 64-bit", r.appImageSize),
            alternative: { url: DEB_URL, label: ".deb for Ubuntu and Debian" },
          },
  };
}

/** "8.9 MB", decimal like Finder. */
export function formatSize(bytes: number): string {
  return `${(Math.round(bytes / 1e5) / 10).toFixed(1)} MB`;
}

export function formatDate(iso: string): string {
  return new Date(iso).toLocaleDateString("en-US", {
    year: "numeric",
    month: "long",
    day: "numeric",
  });
}
