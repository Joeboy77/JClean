import { describe, expect, it } from "vitest";
import { layoutTreemap, neighbour } from "./layout";

describe("layoutTreemap", () => {
  it("fills the box in proportion to value", () => {
    const rects = layoutTreemap(
      [
        { id: "a", value: 60 },
        { id: "b", value: 30 },
        { id: "c", value: 10 },
      ],
      300,
      200,
      0,
    );
    const area = (id: string) => {
      const r = rects.find((x) => x.id === id);
      return r ? r.width * r.height : 0;
    };
    expect(area("a") + area("b") + area("c")).toBeCloseTo(60_000, 0);
    expect(area("a") / 60_000).toBeCloseTo(0.6, 2);
    expect(area("c") / 60_000).toBeCloseTo(0.1, 2);
  });

  it("keeps a gap between cells and stays inside the box", () => {
    const rects = layoutTreemap(
      Array.from({ length: 150 }, (_, i) => ({ id: String(i), value: i + 1 })),
      800,
      600,
      2,
    );
    expect(rects).toHaveLength(150);
    for (const r of rects) {
      expect(r.x).toBeGreaterThanOrEqual(0);
      expect(r.y).toBeGreaterThanOrEqual(0);
      expect(r.x + r.width).toBeLessThanOrEqual(800.001);
      expect(r.y + r.height).toBeLessThanOrEqual(600.001);
    }
  });

  it("skips empty values and empty boxes", () => {
    expect(layoutTreemap([{ id: "a", value: 0 }], 100, 100)).toEqual([]);
    expect(layoutTreemap([{ id: "a", value: 5 }], 0, 100)).toEqual([]);
  });
});

describe("neighbour", () => {
  const rects = [
    { id: "tl", x: 0, y: 0, width: 50, height: 50 },
    { id: "tr", x: 50, y: 0, width: 50, height: 50 },
    { id: "bl", x: 0, y: 50, width: 50, height: 50 },
  ];
  it("moves in the arrow's direction", () => {
    expect(neighbour(rects, "tl", "right")).toBe("tr");
    expect(neighbour(rects, "tl", "down")).toBe("bl");
    expect(neighbour(rects, "tr", "left")).toBe("tl");
    expect(neighbour(rects, "tl", "up")).toBeNull();
  });
});
