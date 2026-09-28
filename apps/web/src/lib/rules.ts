// The rule catalogs for /rules, /rules/windows and /rules/linux, generated from
// rules/<os>/*.json at build time so the pages always match what the app
// detects (spec §12.1).

export type Risk = "safe" | "review" | "caution" | "info";

export interface SiteRule {
  id: string;
  group: string;
  developer: string;
  everyday: string;
  what: string;
  ifCleared: string;
  risk: Risk;
  method: string;
  command: string | null;
  audience: string[];
  locations: string[];
}

export type Os = "macos" | "windows" | "linux";

const macFiles = import.meta.glob<unknown>("../../../../rules/macos/*.json", {
  eager: true,
  import: "default",
});
const windowsFiles = import.meta.glob<unknown>("../../../../rules/windows/*.json", {
  eager: true,
  import: "default",
});

type Json = Record<string, unknown>;
const obj = (v: unknown): Json => (typeof v === "object" && v !== null ? (v as Json) : {});
const str = (v: unknown): string => (typeof v === "string" ? v : "");

function locations(detect: Json): string[] {
  if (Array.isArray(detect.paths)) return detect.paths.map(str);
  if (Array.isArray(detect.folders)) {
    const markers = Array.isArray(detect.markers) ? detect.markers.map(str).join(", ") : "";
    return detect.folders.map((f) => `${str(f)} next to ${markers}`);
  }
  if (typeof detect.probe === "string") return [`Reported by the tool (${detect.probe})`];
  return [];
}

const linuxFiles = import.meta.glob<unknown>("../../../../rules/linux/*.json", {
  eager: true,
  import: "default",
});

const parse = (files: Record<string, unknown>): SiteRule[] =>
  Object.values(files)
    .flatMap((f): unknown[] => (Array.isArray(f) ? (f as unknown[]) : []))
    .map((raw) => {
      const r = obj(raw);
      const labels = obj(r.labels);
      const description = obj(r.description);
      const cleanup = obj(r.cleanup);
      const command = obj(cleanup.command);
      return {
        id: str(r.id),
        group: str(r.group),
        developer: str(labels.developer),
        everyday: str(labels.everyday),
        what: str(description.what),
        ifCleared: str(description.ifCleared),
        risk: str(r.risk) as Risk,
        method: str(cleanup.method),
        command: command.tool
          ? [str(command.tool), ...(Array.isArray(command.args) ? command.args.map(str) : [])].join(
              " ",
            )
          : null,
        audience: Array.isArray(r.audience) ? r.audience.map(str) : [],
        locations: locations(obj(r.detect)),
      };
    })
    .filter((r) => r.id !== "")
    .sort((a, b) => a.group.localeCompare(b.group) || a.developer.localeCompare(b.developer));

export const RULES_BY_OS: Record<Os, SiteRule[]> = {
  macos: parse(macFiles),
  windows: parse(windowsFiles),
  linux: parse(linuxFiles),
};

/** The Mac catalog, the one the home page counts. */
export const RULES = RULES_BY_OS.macos;

export const RISK_LABEL: Record<Risk, string> = {
  safe: "Safe to clean",
  review: "Needs review",
  caution: "Caution",
  info: "Shown for information",
};

export function methodLabel(method: string, os: Os): string {
  const labels: Record<string, string> = {
    delete: "Deleted permanently",
    trash: os === "windows" ? "Moved to the Recycle Bin" : "Moved to the Trash",
    command: "Cleared by its own tool",
    none: "Never cleaned",
  };
  return labels[method] ?? method;
}
