import { describe, expect, it } from "vitest";
import type { MapCellView, Rule, StorageItem } from "../data/types";
import { capCells, cellForItems, foundLevel, usageByCategory, type FoundInput } from "./mapModel";

function rule(
  id: string,
  category: Rule["category"],
  audience: Rule["audience"] = ["developer"],
): Rule {
  return {
    id,
    group: "g",
    labels: { developer: `${id} dev`, everyday: `${id} everyday` },
    description: { what: "w", ifCleared: "i" },
    icon: "package",
    category,
    risk: "safe",
    regenerates: true,
    method: "delete",
    command: null,
    audience,
    docs: null,
    custom: false,
  };
}

function item(
  id: string,
  ruleId: string,
  category: StorageItem["category"],
  bytes: number,
  cleanable = true,
): StorageItem {
  return {
    id,
    ruleId,
    name: id,
    path: `/Users/me/${id}`,
    bytes,
    bytesKnown: true,
    lastUsed: null,
    risk: "safe",
    category,
    method: "delete",
    cleanable,
    blockedReason: null,
    preselected: false,
    mayShareBlocks: false,
    project: null,
  };
}

const rules = new Map([
  ["npm", rule("npm", "developer")],
  ["nm", rule("nm", "developer")],
  ["caches", rule("caches", "apps", ["everyday", "developer"])],
]);
const items = [
  item("a", "npm", "developer", 600),
  item("b", "nm", "developer", 300),
  item("c", "nm", "developer", 200, false),
  item("d", "caches", "apps", 100),
];
const input: FoundInput = { items, rules, audience: "developer", disabledRules: new Set() };

describe("foundLevel", () => {
  it("shows rules at the top, then a rule's items", () => {
    const top = foundLevel(input, "");
    expect(top?.map((c) => [c.id, c.bytes, c.reclaimable, c.drillable, c.itemId])).toEqual([
      ["rule:npm", 600, 600, false, "a"],
      ["rule:nm", 500, 300, true, null],
      ["rule:caches", 100, 100, false, "d"],
    ]);
    expect(top?.[0]?.category).toBe("developer");
    expect(foundLevel(input, "rule:nm")?.map((c) => c.id)).toEqual(["item:b", "item:c"]);
    expect(foundLevel(input, "rule:missing")).toBeNull();
    expect(foundLevel(input, "nope")).toBeNull();
  });

  it("follows Everyday mode and disabled rules", () => {
    const everyday = foundLevel({ ...input, audience: "everyday" }, "");
    expect(everyday?.map((c) => c.id)).toEqual(["rule:caches"]);
    const noNpm = foundLevel({ ...input, disabledRules: new Set(["npm"]) }, "");
    expect(noNpm?.map((c) => c.id)).toEqual(["rule:nm", "rule:caches"]);
  });
});

describe("cellForItems", () => {
  it("finds the item's own cell, else the one containing it", () => {
    const [a, b] = [items[0], items[1]];
    if (!a || !b) throw new Error("fixture");
    const top = foundLevel(input, "") ?? [];
    expect(cellForItems(top, [a])).toBe("rule:npm");
    expect(cellForItems(top, [b])).toBe("rule:nm");
    const inside = foundLevel(input, "rule:nm") ?? [];
    expect(cellForItems(inside, [b])).toBe("item:b");
    const folder: MapCellView = {
      id: "fs:/Users/me",
      name: "me",
      bytes: 1,
      category: "other",
      reclaimable: 0,
      itemId: null,
      drillable: true,
      other: false,
    };
    expect(cellForItems([folder], [a])).toBe("fs:/Users/me");
    // A sibling whose name starts the same doesn't contain it.
    expect(cellForItems([{ ...folder, id: "fs:/Users/m" }], [a])).toBeNull();
  });
});

describe("capCells", () => {
  it("merges the smallest cells past the limit", () => {
    const many: MapCellView[] = Array.from({ length: 160 }, (_, i) => ({
      id: String(i),
      name: String(i),
      bytes: i + 1,
      category: "other",
      reclaimable: 0,
      itemId: null,
      drillable: false,
      other: false,
    }));
    const capped = capCells(many);
    expect(capped).toHaveLength(150);
    expect(capped.at(-1)?.other).toBe(true);
  });
});

describe("usageByCategory", () => {
  it("puts what the scan didn't account for in other", () => {
    const u = usageByCategory(2000, items);
    expect(u.developer).toBe(1100);
    expect(u.other).toBe(800);
  });
  it("scales down when items add up to more than is used", () => {
    const u = usageByCategory(600, items);
    expect(Object.values(u).reduce((a, b) => a + b, 0)).toBeCloseTo(600);
  });
});
