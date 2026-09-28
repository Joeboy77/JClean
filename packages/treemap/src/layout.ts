// Squarified treemap layout. Pure math: d3-hierarchy computes rectangles,
// React renders them (spec stack table).

import { hierarchy, treemap, treemapSquarify } from "d3-hierarchy";

export interface Sized {
  id: string;
  value: number;
}

export interface Rect {
  id: string;
  x: number;
  y: number;
  width: number;
  height: number;
}

/** Lays `items` out in a `width` × `height` box, `gap` pixels apart. */
export function layoutTreemap(
  items: readonly Sized[],
  width: number,
  height: number,
  gap = 2,
): Rect[] {
  const positive = items.filter((i) => i.value > 0);
  if (positive.length === 0 || width <= 0 || height <= 0) return [];
  const root = hierarchy<{ id: string; value: number; children?: Sized[] }>({
    id: "__root__",
    value: 0,
    children: positive.map((i) => ({ id: i.id, value: i.value })),
  })
    .sum((d) => (d.children ? 0 : d.value))
    .sort((a, b) => (b.value ?? 0) - (a.value ?? 0));

  treemap<{ id: string; value: number }>()
    .tile(treemapSquarify.ratio(1.2))
    .size([width + gap, height + gap])
    .paddingInner(0)
    .round(false)(root);

  return root.leaves().map((leaf) => {
    const node = leaf as typeof leaf & { x0: number; y0: number; x1: number; y1: number };
    return {
      id: leaf.data.id,
      x: node.x0,
      y: node.y0,
      width: Math.max(0, node.x1 - node.x0 - gap),
      height: Math.max(0, node.y1 - node.y0 - gap),
    };
  });
}

type Direction = "left" | "right" | "up" | "down";

/** The nearest rectangle in a direction, for arrow-key navigation. */
export function neighbour(
  rects: readonly Rect[],
  fromId: string,
  direction: Direction,
): string | null {
  const from = rects.find((r) => r.id === fromId);
  if (!from) return rects[0]?.id ?? null;
  const cx = from.x + from.width / 2;
  const cy = from.y + from.height / 2;
  let best: string | null = null;
  let bestScore = Number.POSITIVE_INFINITY;
  for (const r of rects) {
    if (r.id === fromId) continue;
    const rx = r.x + r.width / 2;
    const ry = r.y + r.height / 2;
    const dx = rx - cx;
    const dy = ry - cy;
    const ahead =
      direction === "right"
        ? dx > 1
        : direction === "left"
          ? dx < -1
          : direction === "down"
            ? dy > 1
            : dy < -1;
    if (!ahead) continue;
    // Prefer cells straight ahead over ones off to the side.
    const along = direction === "left" || direction === "right" ? Math.abs(dx) : Math.abs(dy);
    const across = direction === "left" || direction === "right" ? Math.abs(dy) : Math.abs(dx);
    const score = along + across * 2;
    if (score < bestScore) {
      bestScore = score;
      best = r.id;
    }
  }
  return best;
}
