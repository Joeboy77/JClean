import { describe, expect, it } from "vitest";
import { ICONS } from "./ruleIcons";

// Every rule file, on every OS: an icon that isn't bundled would quietly
// show as a plain folder.
const files = import.meta.glob<unknown>("../../../../../rules/*/*.json", {
  eager: true,
  import: "default",
});

describe("rule icons", () => {
  it("bundles every icon the rules name", () => {
    const named = Object.values(files)
      .flatMap((f) => (Array.isArray(f) ? (f as unknown[]) : []))
      .map((r) =>
        typeof r === "object" && r !== null ? (r as { icon?: unknown }).icon : undefined,
      )
      .filter((icon): icon is string => typeof icon === "string");
    expect(named.length).toBeGreaterThan(200);
    const missing = [...new Set(named)].filter((icon) => !(icon in ICONS));
    expect(missing).toEqual([]);
  });
});
