// The built-in macOS rule catalog, read straight from rules/macos/*.json so
// labels and descriptions in the UI are always the real ones. From phase 3
// the engine supplies these over IPC.

import type { Audience, Category, Method, Risk, Rule } from "./types";

const files = import.meta.glob<unknown>("../../../../rules/macos/*.json", {
  eager: true,
  import: "default",
});

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
  };
}

export const RULES: readonly Rule[] = Object.values(files)
  .flatMap((file): unknown[] => (Array.isArray(file) ? (file as unknown[]) : []))
  .map(toRule)
  .filter((r): r is Rule => r !== null && r.id !== "");

export const RULES_BY_ID: ReadonlyMap<string, Rule> = new Map(RULES.map((r) => [r.id, r]));
