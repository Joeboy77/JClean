import { AnimatePresence, motion } from "motion/react";
import {
  memo,
  useCallback,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
} from "react";
import { layoutTreemap, neighbour, type Rect } from "./layout";

export interface TreemapCell {
  id: string;
  label: string;
  value: number;
  /** Any CSS colour, e.g. `var(--cat-developer)`. */
  color: string;
  /** 0–1: how much of the cell can be freed. Brighter and hatched when > 0. */
  emphasis?: number;
  /** Accent outline, e.g. the cell's item is selected. */
  outlined?: boolean;
  /** Brightened, e.g. its list row is hovered. */
  highlighted?: boolean;
  /** Can be drilled into. */
  drillable?: boolean;
  /** Read by VoiceOver: "npm cache, 6.1 gigabytes, safe to clean". */
  ariaLabel?: string;
  /** Keeps its place in the layout but isn't drawn yet, so cells revealed
   * one by one appear in their final spots instead of pushing others around. */
  hidden?: boolean;
}

export interface TreemapProps {
  cells: readonly TreemapCell[];
  width: number;
  height: number;
  gap?: number;
  /** Shown under the label in cells big enough for it. */
  formatValue?: (value: number) => string;
  onActivate?: (id: string) => void;
  onHover?: (id: string | null) => void;
  /** Keyboard: Backspace or Escape, usually "go up a level". */
  onBack?: () => void;
  label?: string;
  /** Registers each cell's element, e.g. to draw a line to it. */
  cellRef?: (id: string, el: HTMLElement | null) => void;
}

const spring = { type: "spring", stiffness: 380, damping: 34 } as const;

/** Cell fields are passed as plain values so `memo` can compare them
 * cheaply, whatever object the caller built them in. */
interface CellProps {
  id: string;
  label: string;
  value: number;
  color: string;
  emphasis: number;
  outlined: boolean;
  highlighted: boolean;
  drillable: boolean;
  ariaLabel: string;
  rect: Rect;
  focusable: boolean;
  formatValue: ((value: number) => string) | undefined;
  handlers: Handlers;
}

/** Stable callbacks shared by every cell, so cells only re-render when
 * their own data changes. */
interface Handlers {
  register: (id: string, el: HTMLButtonElement | null) => void;
  focus: (id: string) => void;
  hover: (id: string) => void;
  activate: (id: string) => void;
}

// Memoized: hovering a list row re-renders two cells, not all 150. Motion only
// re-measures cells that re-render, which keeps hovering at 60 fps.
const Cell = memo(function Cell({
  id,
  label,
  value,
  color,
  emphasis,
  outlined,
  highlighted,
  drillable,
  ariaLabel,
  rect,
  focusable,
  formatValue,
  handlers,
}: CellProps) {
  const big = rect.width > 84 && rect.height > 42;
  const small = rect.width > 46 && rect.height > 20;
  return (
    <motion.button
      type="button"
      layout
      ref={(el) => {
        handlers.register(id, el);
      }}
      tabIndex={focusable ? 0 : -1}
      aria-label={ariaLabel}
      onFocus={() => {
        handlers.focus(id);
      }}
      onMouseEnter={() => {
        handlers.hover(id);
      }}
      onClick={() => {
        handlers.activate(id);
      }}
      initial={{ opacity: 0, scale: 0.92 }}
      animate={{ opacity: 1, scale: 1 }}
      exit={{ opacity: 0, scale: 0 }}
      transition={spring}
      data-emphasis={emphasis > 0}
      data-highlighted={highlighted}
      className="jc-cell"
      style={{
        position: "absolute",
        left: rect.x,
        top: rect.y,
        width: rect.width,
        height: rect.height,
        ["--cell-color" as string]: color,
        ["--cell-emphasis" as string]: String(emphasis),
        outline: outlined ? "2px solid var(--accent)" : undefined,
        outlineOffset: outlined ? -2 : undefined,
        cursor: drillable ? "zoom-in" : "default",
      }}
    >
      {small && (
        // The button's aria-label already says this, in words ("gigabytes").
        <span className="jc-cell-label" aria-hidden="true">
          <span className="jc-cell-name">{label}</span>
          {big && formatValue && <span className="jc-cell-value">{formatValue(value)}</span>}
        </span>
      )}
    </motion.button>
  );
});

export function Treemap({
  cells,
  width,
  height,
  gap = 2,
  formatValue,
  onActivate,
  onHover,
  onBack,
  label = "Disk map",
  cellRef,
}: TreemapProps) {
  // Layout depends only on ids and sizes, not on highlight or selection.
  const sizes = cells.map((c) => `${c.id}:${String(c.value)}`).join("|");
  const rects = useMemo(
    () =>
      layoutTreemap(
        cells.map((c) => ({ id: c.id, value: c.value })),
        width,
        height,
        gap,
      ),
    // eslint-disable-next-line react-hooks/exhaustive-deps -- `sizes` stands in for `cells`
    [sizes, width, height, gap],
  );
  const byId = useMemo(() => new Map(cells.map((c) => [c.id, c])), [cells]);
  const shown = useMemo(() => rects.filter((r) => byId.get(r.id)?.hidden !== true), [rects, byId]);
  const [focusId, setFocusId] = useState<string | null>(null);
  const current = focusId && shown.some((r) => r.id === focusId) ? focusId : (shown[0]?.id ?? null);

  // Latest callbacks, read through refs so `handlers` never changes.
  const latest = useRef({ onActivate, onHover, cellRef });
  useLayoutEffect(() => {
    latest.current = { onActivate, onHover, cellRef };
  });
  const elements = useRef(new Map<string, HTMLButtonElement>());
  const handlers = useMemo<Handlers>(
    () => ({
      register: (id, el) => {
        if (el) elements.current.set(id, el);
        else elements.current.delete(id);
        latest.current.cellRef?.(id, el);
      },
      focus: (id) => {
        setFocusId(id);
      },
      hover: (id) => latest.current.onHover?.(id),
      activate: (id) => latest.current.onActivate?.(id),
    }),
    [],
  );

  const move = useCallback((id: string | null) => {
    if (!id) return;
    setFocusId(id);
    elements.current.get(id)?.focus();
  }, []);

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (!current) return;
    const dir = {
      ArrowLeft: "left",
      ArrowRight: "right",
      ArrowUp: "up",
      ArrowDown: "down",
    } as const;
    const d = dir[e.key as keyof typeof dir] as (typeof dir)[keyof typeof dir] | undefined;
    if (d) {
      move(neighbour(shown, current, d));
      e.preventDefault();
    } else if ((e.key === "Backspace" || e.key === "Escape") && onBack) {
      onBack();
      e.preventDefault();
    }
  };

  return (
    <div
      role="group"
      aria-label={label}
      onKeyDown={onKeyDown}
      onMouseLeave={() => onHover?.(null)}
      style={{ position: "relative", width, height }}
    >
      <AnimatePresence initial={false}>
        {shown.map((r) => {
          const cell = byId.get(r.id);
          if (!cell) return null;
          return (
            <Cell
              key={r.id}
              id={cell.id}
              label={cell.label}
              value={cell.value}
              color={cell.color}
              emphasis={Math.max(0, Math.min(1, cell.emphasis ?? 0))}
              outlined={cell.outlined === true}
              highlighted={cell.highlighted === true}
              drillable={cell.drillable === true}
              ariaLabel={cell.ariaLabel ?? cell.label}
              rect={r}
              focusable={r.id === current}
              formatValue={formatValue}
              handlers={handlers}
            />
          );
        })}
      </AnimatePresence>
    </div>
  );
}
