// The disk map's cells for the "found" view (rules → items),
// built from the list items so it fills in live while a scan runs. The
// full-scan folder view comes from the engine one level at a time.

import {
  CATEGORIES,
  type Audience,
  type Category,
  type MapCellView,
  type Rule,
  type StorageItem,
} from "../data/types";
import { itemName, ruleLabel, visibleIn } from "./selectors";

/** Most cells at one level; the smallest merge into one (spec §5.2). */
export const MAX_CELLS = 150;

export const CATEGORY_NAME: Record<Category, string> = {
  apps: "Apps",
  developer: "Developer",
  system: "System",
  media: "Media",
  documents: "Documents",
  other: "Other",
};

function reclaimable(item: StorageItem): number {
  return item.cleanable ? item.bytes : 0;
}

export function capCells(cells: MapCellView[], max = MAX_CELLS): MapCellView[] {
  const sorted = cells
    .filter((c) => c.bytes > 0)
    .sort((a, b) => b.bytes - a.bytes || a.name.localeCompare(b.name));
  if (sorted.length <= max) return sorted;
  const kept = sorted.slice(0, max - 1);
  const rest = sorted.slice(max - 1);
  kept.push({
    id: "other",
    name: `Other small items (${String(rest.length)})`,
    bytes: rest.reduce((n, c) => n + c.bytes, 0),
    category: "other",
    reclaimable: rest.reduce((n, c) => n + c.reclaimable, 0),
    itemId: null,
    drillable: false,
    other: true,
  });
  return kept;
}

export interface FoundInput {
  items: readonly StorageItem[];
  rules: ReadonlyMap<string, Rule>;
  audience: Audience;
  disabledRules: ReadonlySet<string>;
}

function visibleItems({ items, rules, audience, disabledRules }: FoundInput): StorageItem[] {
  return items.filter((i) => {
    const rule = rules.get(i.ruleId);
    return rule !== undefined && !disabledRules.has(rule.id) && visibleIn(rule, audience);
  });
}

/** One level of the found map: every rule at the top (like the spec's
 * mockup: Xcode, Docker, node_modules…), then a rule's items. `""` is the
 * top. `null` if the ID doesn't exist. */
export function foundLevel(input: FoundInput, id: string): MapCellView[] | null {
  const items = visibleItems(input);
  if (id === "") {
    const byRule = new Map<string, StorageItem[]>();
    for (const item of items) byRule.set(item.ruleId, [...(byRule.get(item.ruleId) ?? []), item]);
    return capCells(
      [...byRule].map(([ruleId, list]) => {
        const rule = input.rules.get(ruleId);
        const only = list.length === 1 ? list[0] : undefined;
        return {
          id: `rule:${ruleId}`,
          name: rule ? ruleLabel(rule, input.audience) : ruleId,
          bytes: list.reduce((n, i) => n + i.bytes, 0),
          category: rule?.category ?? list[0]?.category ?? "other",
          reclaimable: list.reduce((n, i) => n + reclaimable(i), 0),
          itemId: only?.id ?? null,
          drillable: !only,
          other: false,
        };
      }),
    );
  }
  if (id.startsWith("rule:")) {
    const ruleId = id.slice(5);
    const inRule = items.filter((i) => i.ruleId === ruleId);
    if (inRule.length === 0) return null;
    return capCells(
      inRule.map((i) => ({
        id: `item:${i.id}`,
        name: itemName(i),
        bytes: i.bytes,
        category: i.category,
        reclaimable: reclaimable(i),
        itemId: i.id,
        drillable: false,
        other: false,
      })),
    );
  }
  return null;
}

/** The cell at this level that holds any of `items`: the item's own cell,
 * or the category, rule or folder containing it. */
export function cellForItems(
  cells: readonly MapCellView[],
  items: readonly StorageItem[],
): string | null {
  for (const item of items) {
    const own = cells.find((c) => c.itemId === item.id);
    if (own) return own.id;
  }
  for (const item of items) {
    const hit = cells.find(
      (c) =>
        c.id === `rule:${item.ruleId}` ||
        (c.id.startsWith("fs:") &&
          item.path !== null &&
          (item.path === c.id.slice(3) || item.path.startsWith(`${c.id.slice(3)}/`))),
    );
    if (hit) return hit.id;
  }
  return null;
}

/** Used space by category for the capacity bar. What the scan didn't
 * account for (macOS itself, apps, documents it didn't walk) is "other". */
export function usageByCategory(
  used: number,
  items: readonly StorageItem[],
): Record<Category, number> {
  const out: Record<Category, number> = {
    apps: 0,
    developer: 0,
    system: 0,
    media: 0,
    documents: 0,
    other: 0,
  };
  for (const item of items) out[item.category] += item.bytes;
  const known = CATEGORIES.reduce((n, c) => n + out[c], 0);
  if (known > used) {
    // Hard links and clones can make items add up to more than the disk uses.
    for (const c of CATEGORIES) out[c] = (out[c] / known) * used;
  } else {
    out.other += used - known;
  }
  return out;
}
