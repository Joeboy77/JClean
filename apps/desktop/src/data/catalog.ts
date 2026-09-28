// The built-in rule catalog for this OS, read straight from rules/<os>/*.json.
// Used by the browser build and as the starting point in the app, where the
// engine then supplies the full set (built-in plus custom) over IPC.

import type { Audience, Category, Method, Risk, Rule } from "./types";
import { isWindows } from "../lib/platform";

const macFiles = import.meta.glob<unknown>("../../../../rules/macos/*.json", {
  eager: true,
  import: "default",
});
const windowsFiles = import.meta.glob<unknown>("../../../../rules/windows/*.json", {
  eager: true,
  import: "default",
});
const files = isWindows ? windowsFiles : macFiles;

type Json = Record<string, unknown>;

function isObject(v: unknown): v is Json {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

function str(v: unknown): string {
  return typeof v === "string" ? v : "";
}

function toRule(raw: unknown): Rule | null {
  if (!isObject(raw)) return null;
  const labels = isObject(raw.labels) ? raw.labels : {};
  const description = isObject(raw.description) ? raw.description : {};
  const cleanup = isObject(raw.cleanup) ? raw.cleanup : {};
  const command = isObject(cleanup.command) ? cleanup.command : null;
  const args = command && Array.isArray(command.args) ? command.args.map(str) : [];
  return {
    id: str(raw.id),
    group: str(raw.group),
    labels: { developer: str(labels.developer), everyday: str(labels.everyday) },
    description: { what: str(description.what), ifCleared: str(description.ifCleared) },
    icon: str(raw.icon),
    category: str(raw.category) as Category,
    risk: str(raw.risk) as Risk,
    regenerates: raw.regenerates === true,
    method: str(cleanup.method) as Method,
    command: command ? [str(command.tool), ...args].join(" ") : null,
    audience: Array.isArray(raw.audience) ? (raw.audience.map(str) as Audience[]) : [],
    docs: typeof raw.docs === "string" ? raw.docs : null,
    custom: false,
  };
}

export const RULES: readonly Rule[] = Object.values(files)
  .flatMap((file): unknown[] => (Array.isArray(file) ? (file as unknown[]) : []))
  .map(toRule)
  .filter((r): r is Rule => r !== null && r.id !== "");

export const RULES_BY_ID: ReadonlyMap<string, Rule> = new Map(RULES.map((r) => [r.id, r]));
