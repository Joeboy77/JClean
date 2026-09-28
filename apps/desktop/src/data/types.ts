// Domain types the UI works with. Phase 3 maps the engine's IPC types onto these.

export type Risk = "safe" | "review" | "caution" | "info";
export type Category = "apps" | "developer" | "system" | "media" | "documents" | "other";
export type Method = "delete" | "trash" | "command" | "none";
export type Audience = "everyday" | "developer";

export const RISKS: readonly Risk[] = ["safe", "review", "caution", "info"];
export const CATEGORIES: readonly Category[] = [
  "apps",
  "developer",
  "system",
  "media",
  "documents",
  "other",
];

export interface Rule {
  id: string;
  group: string;
  labels: { developer: string; everyday: string };
  description: { what: string; ifCleared: string };
  icon: string;
  category: Category;
  risk: Risk;
  regenerates: boolean;
  method: Method;
  /** e.g. `npm cache clean --force`, for command cleanups. */
  command: string | null;
  audience: Audience[];
  docs: string | null;
}

export interface ProjectRef {
  name: string;
  root: string;
  active: boolean;
}

export interface StorageItem {
  id: string;
  ruleId: string;
  /** Item-specific name shown with the rule label (a cache's folder, a project). */
  name: string | null;
  path: string | null;
  bytes: number;
  /** False when a tool can't report the size (e.g. local snapshots). */
  bytesKnown: boolean;
  /** Unix seconds. */
  lastUsed: number | null;
  risk: Risk;
  category: Category;
  method: Method;
  cleanable: boolean;
  blockedReason: string | null;
  preselected: boolean;
  mayShareBlocks: boolean;
  project: ProjectRef | null;
}

export interface Volume {
  name: string;
  total: number;
  /** What Finder shows as available: includes purgeable space. */
  available: number;
  /** Space macOS frees on its own when needed. */
  purgeable: number;
}

/** A cell of the disk map, from either map view. */
export interface MapCellView {
  id: string;
  name: string;
  bytes: number;
  category: Category;
  reclaimable: number;
  /** Set when the cell is exactly one list item. */
  itemId: string | null;
  drillable: boolean;
  /** The merged "Other small items" cell. */
  other: boolean;
}

/** "found": what JClean found, by category → rule → item. "folders": the full scan's folder tree. */
export type MapView = "found" | "folders";

export type ScanPhase = "idle" | "scanning" | "results" | "cleaning" | "done";
