// Release facts read at build time (spec §12.2). Each release triggers a
// rebuild through the Cloudflare deploy hook, so the site never goes stale.
// If GitHub can't be reached, the site still builds with safe fallbacks.

export const REPO = "Joeboy77/JClean";
export const DMG = "JClean_macos_universal.dmg";
/** Stable URL: always the newest release's DMG. */
export const DMG_URL = `https://github.com/${REPO}/releases/latest/download/${DMG}`;
export const RELEASES_URL = `https://github.com/${REPO}/releases`;

export interface Checksum {
  sha256: string;
  file: string;
}

export interface LatestRelease {
  version: string | null;
  /** Bytes. */
  dmgSize: number | null;
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

/** The DMG's size from its download headers. */
async function dmgSize(): Promise<number | null> {
  try {
    const res = await fetch(DMG_URL, { method: "HEAD", redirect: "follow" });
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
    const dmg = assets.find((a) => a.name === DMG);
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
      dmgSize: dmg && typeof dmg.size === "number" ? dmg.size : await dmgSize(),
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
