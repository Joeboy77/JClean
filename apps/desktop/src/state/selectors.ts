// Pure functions that turn scan results + UI state into what the sidebar
// shows. Kept free of React and the store so they're easy to test.

import type { Audience, Risk, Rule, StorageItem } from "../data/types";
import { RISKS } from "../data/types";
import { basename } from "../lib/format";

export interface Filters {
  developer: boolean;
  system: boolean;
  large: boolean;
  safeOnly: boolean;
}

export type FilterKey = keyof Filters;

export const NO_FILTERS: Filters = {
  developer: false,
  system: false,
  large: false,
  safeOnly: false,
};

/** The "Large files" filter: items of 1 GB or more. */
export const LARGE_BYTES = 1e9;

export type Check = "all" | "some" | "none";

export function visibleIn(rule: Rule, audience: Audience): boolean {
  return audience === "developer" || rule.audience.includes("everyday");
}

export function ruleLabel(rule: Rule, audience: Audience): string {
  return audience === "developer" ? rule.labels.developer : rule.labels.everyday;
}

/** A child row's label: its own name, else the last part of its path. */
export function itemName(item: StorageItem): string {
  return item.name ?? (item.path ? basename(item.path) : "");
}

/** Developer (category), System (category) and Large (size) widen the list
 * together; Safe only narrows whatever is shown. */
export function matchesFilters(item: StorageItem, filters: Filters): boolean {
  const kinds = filters.developer || filters.system || filters.large;
  if (kinds) {
    const hit =
      (filters.developer && item.category === "developer") ||
      (filters.system && item.category === "system") ||
      (filters.large && item.bytes >= LARGE_BYTES);
    if (!hit) return false;
  }
  return !filters.safeOnly || item.risk === "safe";
}

/** Search across names, paths, both label sets and tool names. */
export function matchesSearch(item: StorageItem, rule: Rule, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  return [
    item.name,
    item.path,
    item.project?.name,
    rule.labels.developer,
    rule.labels.everyday,
    rule.group,
    rule.command,
  ].some((field) => field?.toLowerCase().includes(q) === true);
}

export function filterCounts(items: readonly StorageItem[]): Record<FilterKey, number> {
  const counts = { developer: 0, system: 0, large: 0, safeOnly: 0 };
  for (const item of items) {
    if (item.category === "developer") counts.developer++;
    if (item.category === "system") counts.system++;
    if (item.bytes >= LARGE_BYTES) counts.large++;
    if (item.risk === "safe") counts.safeOnly++;
  }
  return counts;
}

export function checkState(ids: readonly string[], selected: ReadonlySet<string>): Check {
  let n = 0;
  for (const id of ids) if (selected.has(id)) n++;
  if (n === 0) return "none";
  return n === ids.length ? "all" : "some";
}

export function selectedBytes(
  items: readonly StorageItem[],
  selected: ReadonlySet<string>,
): number {
  let total = 0;
  for (const item of items) if (selected.has(item.id)) total += item.bytes;
  return total;
}

export interface SectionRow {
  kind: "section";
  key: string;
  risk: Risk;
  count: number;
  bytes: number;
  collapsed: boolean;
}

export interface GroupRow {
  kind: "group";
  key: string;
  risk: Risk;
  rule: Rule;
  items: StorageItem[];
  bytes: number;
  lastUsed: number | null;
  expandable: boolean;
  expanded: boolean;
  check: Check;
  /** Nothing in the group can be cleaned. */
  disabled: boolean;
}

export interface ItemRow {
  kind: "item";
  key: string;
  item: StorageItem;
  rule: Rule;
  checked: boolean;
}

export interface SkeletonRow {
  kind: "skeleton";
  key: string;
}

/** A rule whose folders macOS kept from us (spec §11). */
export interface LockedRow {
  kind: "locked";
  key: string;
  rule: Rule;
}

export type Row = SectionRow | GroupRow | ItemRow | SkeletonRow | LockedRow;

export interface ListInput {
  items: readonly StorageItem[];
  /** Everyday mode shows only rules meant for everyone (spec §2). */
  audience: Audience;
  rules: ReadonlyMap<string, Rule>;
  filters: Filters;
  search: string;
  selected: ReadonlySet<string>;
  expanded: ReadonlySet<string>;
  collapsed: ReadonlySet<Risk>;
  disabledRules: ReadonlySet<string>;
  /** Rules that need Full Disk Access to be measured. */
  locked?: readonly string[];
}

export function groupKey(risk: Risk, ruleId: string): string {
  return `group:${risk}:${ruleId}`;
}

/** Flattens everything visible into rows for the virtual list. */
export function buildRows(input: ListInput): Row[] {
  const { rules, audience, filters, search, selected, expanded, collapsed, disabledRules } = input;
  const bySection = new Map<Risk, Map<string, StorageItem[]>>();
  for (const item of input.items) {
    const rule = rules.get(item.ruleId);
    if (!rule || disabledRules.has(rule.id) || !visibleIn(rule, audience)) continue;
    if (!matchesFilters(item, filters) || !matchesSearch(item, rule, search)) continue;
    const section = bySection.get(item.risk) ?? new Map<string, StorageItem[]>();
    const group = section.get(rule.id) ?? [];
    group.push(item);
    section.set(rule.id, group);
    bySection.set(item.risk, section);
  }

  const lockedByRisk = new Map<Risk, Rule[]>();
  for (const id of input.locked ?? []) {
    const rule = rules.get(id);
    if (
      !rule ||
      disabledRules.has(id) ||
      !visibleIn(rule, audience) ||
      filters.safeOnly ||
      search.trim()
    )
      continue;
    lockedByRisk.set(rule.risk, [...(lockedByRisk.get(rule.risk) ?? []), rule]);
  }

  const rows: Row[] = [];
  for (const risk of RISKS) {
    const locked = lockedByRisk.get(risk) ?? [];
    const section =
      bySection.get(risk) ?? (locked.length ? new Map<string, StorageItem[]>() : undefined);
    if (!section) continue;
    const groups: GroupRow[] = [];
    for (const [ruleId, items] of section) {
      const rule = rules.get(ruleId);
      if (!rule) continue;
      items.sort((a, b) => b.bytes - a.bytes);
      const key = groupKey(risk, ruleId);
      const cleanable = items.filter((i) => i.cleanable).map((i) => i.id);
      groups.push({
        kind: "group",
        key,
        risk,
        rule,
        items,
        bytes: items.reduce((sum, i) => sum + i.bytes, 0),
        lastUsed: items.reduce<number | null>(
          (max, i) =>
            i.lastUsed !== null && (max === null || i.lastUsed > max) ? i.lastUsed : max,
          null,
        ),
        expandable: items.length > 1,
        expanded: expanded.has(key),
        check: cleanable.length ? checkState(cleanable, selected) : "none",
        disabled: cleanable.length === 0,
      });
    }
    groups.sort((a, b) => b.bytes - a.bytes);

    const isCollapsed = collapsed.has(risk);
    rows.push({
      kind: "section",
      key: `section:${risk}`,
      risk,
      count: groups.reduce((n, g) => n + g.items.length, 0),
      bytes: groups.reduce((n, g) => n + g.bytes, 0),
      collapsed: isCollapsed,
    });
    if (isCollapsed) continue;
    for (const rule of locked) rows.push({ kind: "locked", key: `locked:${rule.id}`, rule });
    for (const group of groups) {
      rows.push(group);
      if (group.expandable && group.expanded) {
        for (const item of group.items) {
          rows.push({
            kind: "item",
            key: `item:${item.id}`,
            item,
            rule: group.rule,
            checked: selected.has(item.id),
          });
        }
      }
    }
  }
  return rows;
}

/** "23 projects", "4 folders", "1 item". */
export function countLabel(items: readonly StorageItem[]): string {
  const n = items.length;
  if (items.every((i) => i.project)) return `${String(n)} ${n === 1 ? "project" : "projects"}`;
  return `${String(n)} ${n === 1 ? "item" : "items"}`;
}
