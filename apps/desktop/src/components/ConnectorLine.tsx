import { motion } from "motion/react";
import { useMemo } from "react";
import { cellElement } from "../lib/cellRegistry";
import { useStore } from "../state/store";

interface Line {
  x1: number;
  y1: number;
  x2: number;
  y2: number;
}

/** A thin line from the hovered row to its map cell (spec §5.8, moment 4),
 * echoing Proton's line from home to server. */
export function ConnectorLine() {
  const hover = useStore((s) => s.hover);
  const cellId = useStore((s) => s.linkCell);
  // Both elements are on screen by the time the hover state changes, so
  // measuring during render is safe here.
  const line = useMemo<Line | null>(() => {
    const row = hover
      ? document.querySelector<HTMLElement>(`[data-row-key="${CSS.escape(hover.rowKey)}"]`)
      : null;
    const cell = cellId ? cellElement(cellId) : undefined;
    if (!row || !cell) return null;
    const r = row.getBoundingClientRect();
    const c = cell.getBoundingClientRect();
    return {
      x1: r.right - 4,
      y1: r.top + r.height / 2,
      x2: c.left + Math.min(24, c.width / 2),
      y2: c.top + c.height / 2,
    };
  }, [hover, cellId]);

  if (!line) return null;
  const mid = (line.x1 + line.x2) / 2;
  return (
    <svg className="pointer-events-none fixed inset-0 z-20 h-full w-full" aria-hidden="true">
      <motion.path
        key={`${String(line.x2)}-${String(line.y2)}-${String(line.y1)}`}
        d={`M ${String(line.x1)} ${String(line.y1)} C ${String(mid)} ${String(line.y1)}, ${String(mid)} ${String(line.y2)}, ${String(line.x2)} ${String(line.y2)}`}
        fill="none"
        stroke="var(--accent)"
        strokeWidth={1.5}
        strokeLinecap="round"
        initial={{ pathLength: 0, opacity: 0 }}
        animate={{ pathLength: 1, opacity: 0.9 }}
        transition={{ duration: 0.2, ease: "easeOut" }}
      />
      <motion.circle
        cx={line.x2}
        cy={line.y2}
        r={3}
        fill="var(--accent)"
        initial={{ scale: 0 }}
        animate={{ scale: 1 }}
        transition={{ delay: 0.15, duration: 0.1 }}
      />
    </svg>
  );
}
