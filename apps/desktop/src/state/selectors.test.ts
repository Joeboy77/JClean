import { describe, expect, it } from "vitest";
import type { Rule, StorageItem } from "../data/types";
import {
  NO_FILTERS,
  buildRows,
  checkState,
  countLabel,
  filterCounts,
  groupKey,
  matchesFilters,
  matchesSearch,
  selectedBytes,
  visibleItems,
  type GroupRow,
  type ListInput,
} from "./selectors";

function rule(id: string, overrides: Partial<Rule> = {}): Rule {
  return {
    id,
    group: "Package caches",
    labels: { developer: `${id} dev`, everyday: `${id} everyday` },
    description: { what: "w", ifCleared: "i" },
    icon: "package",
    category: "developer",
    risk: "safe",
    regenerates: true,
    method: "delete",
    command: null,
    audience: ["developer"],
    docs: null,
    custom: false,
    ...overrides,
  };
}

function item(id: string, ruleId: string, overrides: Partial<StorageItem> = {}): StorageItem {
  return {
    id,
    ruleId,
    name: null,
    path: `/Users/me/${id}`,
    bytes: 1e6,
    bytesKnown: true,
    lastUsed: null,
    risk: "safe",
    category: "developer",
    method: "delete",
    cleanable: true,
    blockedReason: null,
    preselected: false,
    mayShareBlocks: false,
    project: null,
    ...overrides,
  };
}

const rules = new Map([
  ["npm", rule("npm", { command: "npm cache clean --force" })],
  ["nm", rule("nm")],
  ["logs", rule("logs", { category: "system" })],
]);

function input(items: StorageItem[], overrides: Partial<ListInput> = {}): ListInput {
  return {
    items,
    audience: "developer",
    rules,
    filters: NO_FILTERS,
    search: "",
    selected: new Set(),
    expanded: new Set(),
    collapsed: new Set(),
    disabledRules: new Set(),
    ...overrides,
  };
}

describe("buildRows", () => {
  const items = [
    item("a", "npm", { bytes: 6e9 }),
    item("b", "nm", { bytes: 1e9, project: { name: "old", root: "/x", active: false } }),
    item("c", "nm", { bytes: 2e9, project: { name: "older", root: "/y", active: false } }),
    item("d", "nm", {
      risk: "review",
      bytes: 3e8,
      project: { name: "live", root: "/z", active: true },
    }),
    item("e", "logs", { risk: "info", category: "system", cleanable: false }),
  ];

  it("groups by risk section, then rule, largest first", () => {
    const rows = buildRows(input(items));
    expect(rows.map((r) => r.key)).toEqual([
      "section:safe",
      "group:safe:npm",
      "group:safe:nm",
      "section:review",
      "group:review:nm",
      "section:info",
      "group:info:logs",
    ]);
    const safe = rows[0];
    expect(safe?.kind === "section" && safe.count).toBe(3);
  });

  it("expands groups into their items and collapses sections", () => {
    const rows = buildRows(
      input(items, { expanded: new Set([groupKey("safe", "nm")]), collapsed: new Set(["info"]) }),
    );
    expect(rows.map((r) => r.key)).toEqual([
      "section:safe",
      "group:safe:npm",
      "group:safe:nm",
      "item:c",
      "item:b",
      "section:review",
      "group:review:nm",
      "section:info",
    ]);
  });

  it("reports tri-state selection and disables groups with nothing cleanable", () => {
    const rows = buildRows(input(items, { selected: new Set(["b"]) }));
    const nm = rows.find((r) => r.key === "group:safe:nm") as GroupRow;
    expect(nm.check).toBe("some");
    const logs = rows.find((r) => r.key === "group:info:logs") as GroupRow;
    expect(logs.disabled).toBe(true);
  });

  it("shows only everyday rules in Everyday mode", () => {
    const withEveryday = new Map(rules);
    withEveryday.set(
      "logs",
      rule("logs", { category: "system", audience: ["everyday", "developer"] }),
    );
    const rows = buildRows(input(items, { audience: "everyday", rules: withEveryday }));
    expect(rows.map((r) => r.key)).toEqual(["section:info", "group:info:logs"]);
  });

  it("shows rules that need Full Disk Access instead of hiding them", () => {
    const rows = buildRows(input(items, { locked: ["npm"] }));
    expect(rows.filter((r) => r.kind === "locked").map((r) => r.key)).toEqual(["locked:npm"]);
    const empty = buildRows(input([], { locked: ["npm"] }));
    expect(empty.map((r) => r.key)).toEqual(["section:safe", "locked:npm"]);
  });

  it("lists exactly the items the rows show", () => {
    const shown = visibleItems(input(items, { disabledRules: new Set(["npm"]), search: "b" }));
    expect(shown.map((i) => i.id)).toEqual(["b"]);
  });

  it("hides disabled rules", () => {
    const rows = buildRows(input(items, { disabledRules: new Set(["npm"]) }));
    expect(rows.some((r) => r.key === "group:safe:npm")).toBe(false);
  });

  it("handles 5,000 items quickly", () => {
    const many = Array.from({ length: 5000 }, (_, i) =>
      item(String(i), i % 2 ? "nm" : "npm", { bytes: i * 1000 }),
    );
    const start = performance.now();
    const rows = buildRows(
      input(many, { expanded: new Set([groupKey("safe", "nm"), groupKey("safe", "npm")]) }),
    );
    expect(rows.length).toBe(5003);
    expect(performance.now() - start).toBeLessThan(100);
  });
});

describe("filters and search", () => {
  it("widens with kinds and narrows with safe only", () => {
    const dev = item("x", "npm");
    const sys = item("y", "logs", { category: "system", risk: "review" });
    const big = item("z", "logs", { category: "system", bytes: 5e9 });
    expect(matchesFilters(dev, { ...NO_FILTERS, developer: true })).toBe(true);
    expect(matchesFilters(sys, { ...NO_FILTERS, developer: true })).toBe(false);
    expect(matchesFilters(sys, { ...NO_FILTERS, developer: true, system: true })).toBe(true);
    expect(matchesFilters(sys, { ...NO_FILTERS, system: true, safeOnly: true })).toBe(false);
    expect(matchesFilters(big, { ...NO_FILTERS, large: true })).toBe(true);
    expect(filterCounts([dev, sys, big])).toEqual({
      developer: 1,
      system: 2,
      large: 1,
      safeOnly: 2,
    });
  });

  it("searches names, paths, labels and tool names", () => {
    const npm = rules.get("npm");
    if (!npm) throw new Error("fixture");
    const i = item("q", "npm", { path: "/Users/me/.npm/_cacache" });
    expect(matchesSearch(i, npm, "_cacache")).toBe(true);
    expect(matchesSearch(i, npm, "NPM CACHE CLEAN")).toBe(true);
    expect(matchesSearch(i, npm, "everyday")).toBe(true);
    expect(matchesSearch(i, npm, "docker")).toBe(false);
  });
});

describe("selection", () => {
  it("computes state and totals", () => {
    const sel = new Set(["a"]);
    expect(checkState(["a", "b"], sel)).toBe("some");
    expect(checkState(["a"], sel)).toBe("all");
    expect(checkState(["b"], sel)).toBe("none");
    expect(
      selectedBytes([item("a", "npm", { bytes: 5 }), item("b", "npm", { bytes: 7 })], sel),
    ).toBe(5);
  });

  it("labels counts", () => {
    expect(
      countLabel([item("a", "nm", { project: { name: "p", root: "/p", active: false } })]),
    ).toBe("1 project");
    expect(countLabel([item("a", "npm"), item("b", "npm")])).toBe("2 items");
  });
});
