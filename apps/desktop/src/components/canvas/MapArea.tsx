import { Treemap, type TreemapCell } from "@jclean/treemap";
import { ChevronRight } from "lucide-react";
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useShallow } from "zustand/react/shallow";
import type { MapCellView, StorageItem } from "../../data/types";
import { formatBytes, speakBytes } from "../../lib/format";
import { registerCell } from "../../lib/cellRegistry";
import { folderLevel } from "../../state/engine";
import { cellForItems, foundLevel } from "../../state/mapModel";
import { useStore } from "../../state/store";
import { HoverCard } from "./HoverCard";

function useSize<T extends HTMLElement>() {
  const ref = useRef<T>(null);
  const [size, setSize] = useState({ width: 0, height: 0 });
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const observer = new ResizeObserver(([entry]) => {
      if (entry) setSize({ width: entry.contentRect.width, height: entry.contentRect.height });
    });
    observer.observe(el);
    return () => {
      observer.disconnect();
    };
  }, []);
  return [ref, size] as const;
}

function ariaFor(cell: MapCellView): string {
  const parts = [cell.name, speakBytes(cell.bytes)];
  if (cell.reclaimable > 0) parts.push(`${speakBytes(cell.reclaimable)} can be freed`);
  if (cell.drillable) parts.push("press Enter to open");
  return parts.join(", ");
}

/** The disk map with breadcrumb, zoom and hover card (spec §5.2). */
export function MapArea() {
  const s = useStore(
    useShallow((st) => ({
      phase: st.phase,
      items: st.items,
      audience: st.audience,
      disabledRules: st.disabledRules,
      rules: st.rules,
      selected: st.selected,
      hasTree: st.hasTree,
      mapView: st.mapView,
      mapPath: st.mapPath,
      zoom: st.zoom,
      hover: st.hover,
      volume: st.volume,
    })),
  );
  const { drillIn, goToCrumb, setMapView, openDrawer, setLinkCell } = useStore(
    useShallow((st) => ({
      drillIn: st.drillIn,
      goToCrumb: st.goToCrumb,
      setMapView: st.setMapView,
      openDrawer: st.openDrawer,
      setLinkCell: st.setLinkCell,
    })),
  );
  const levelId = s.mapPath.at(-1)?.id ?? "";
  const [folderCells, setFolderCells] = useState<{ id: string; cells: MapCellView[] } | null>(null);
  const [folderError, setFolderError] = useState<string | null>(null);
  const [hoverCell, setHoverCell] = useState<string | null>(null);
  const [boxRef, box] = useSize<HTMLDivElement>();

  // The folder view comes from the engine one level at a time (spec §15).
  useEffect(() => {
    if (s.mapView !== "folders" || s.phase === "scanning") return;
    let stale = false;
    folderLevel(levelId).then(
      (cells) => {
        if (!stale) {
          setFolderCells({ id: levelId, cells });
          setFolderError(null);
        }
      },
      (e: unknown) => {
        if (!stale) setFolderError(e instanceof Error ? e.message : String(e));
      },
    );
    return () => {
      stale = true;
    };
  }, [s.mapView, levelId, s.phase]);

  const cells = useMemo<MapCellView[]>(() => {
    if (s.mapView === "folders") return folderCells?.id === levelId ? folderCells.cells : [];
    return (
      foundLevel(
        {
          items: s.items,
          rules: s.rules,
          audience: s.audience,
          disabledRules: s.disabledRules,
        },
        levelId,
      ) ?? []
    );
  }, [s.mapView, s.items, s.rules, s.audience, s.disabledRules, folderCells, levelId]);

  const itemsById = useMemo(() => new Map(s.items.map((i) => [i.id, i])), [s.items]);
  const hoveredItems = useMemo<StorageItem[]>(
    () =>
      s.hover
        ? s.hover.itemIds.map((id) => itemsById.get(id)).filter((i): i is StorageItem => !!i)
        : [],
    [s.hover, itemsById],
  );
  const linked = useMemo(
    () => (hoveredItems.length ? cellForItems(cells, hoveredItems) : null),
    [cells, hoveredItems],
  );
  useEffect(() => {
    setLinkCell(linked);
  }, [linked, setLinkCell]);

  const treemapCells = useMemo<TreemapCell[]>(
    () =>
      cells.map((c) => ({
        id: c.id,
        label: c.name,
        value: c.bytes,
        color: `var(--cat-${c.category})`,
        emphasis: c.bytes > 0 ? c.reclaimable / c.bytes : 0,
        outlined: c.itemId !== null && s.selected.has(c.itemId),
        highlighted: c.id === linked,
        drillable: c.drillable,
        ariaLabel: ariaFor(c),
      })),
    [cells, s.selected, linked],
  );

  const shownCell = cells.find((c) => c.id === (hoverCell ?? linked)) ?? null;
  const shownItem = shownCell?.itemId ? (itemsById.get(shownCell.itemId) ?? null) : null;
  const rootName = s.mapView === "folders" ? "Home" : "Everything found";
  const crumbs = [{ id: "", name: rootName }, ...s.mapPath];
  const scanning = s.phase === "scanning";

  return (
    <div className="flex h-full min-h-0 flex-col gap-3">
      <div className="flex min-h-7 items-center gap-3">
        <nav aria-label="Map location" className="flex min-w-0 flex-1 items-center gap-1 text-sm">
          {crumbs.map((c, i) => {
            const last = i === crumbs.length - 1;
            return (
              <span key={`${c.id}-${String(i)}`} className="flex min-w-0 items-center gap-1">
                {i > 0 && (
                  <ChevronRight size={13} className="shrink-0 text-muted" aria-hidden="true" />
                )}
                <button
                  type="button"
                  disabled={last}
                  aria-current={last ? "location" : undefined}
                  onClick={() => {
                    goToCrumb(i - 1);
                  }}
                  className={`truncate rounded-row px-1.5 py-0.5 ${last ? "text-text" : "text-muted hover:bg-raised hover:text-text"}`}
                >
                  {c.name}
                </button>
              </span>
            );
          })}
        </nav>
        {s.hasTree && (
          <div
            role="radiogroup"
            aria-label="Map view"
            className="flex rounded-control border border-line bg-surface p-0.5 text-xs"
          >
            {(["found", "folders"] as const).map((v) => (
              <button
                key={v}
                type="button"
                role="radio"
                aria-checked={s.mapView === v}
                onClick={() => {
                  setMapView(v);
                }}
                className={`rounded-[6px] px-2.5 py-1 ${s.mapView === v ? "bg-raised text-text" : "text-muted hover:text-text"}`}
              >
                {v === "found" ? "What JClean found" : "All folders"}
              </button>
            ))}
          </div>
        )}
      </div>

      <div ref={boxRef} className="scroll-area relative min-h-0 flex-1 overflow-auto rounded-card">
        {box.width > 0 && box.height > 0 && (
          <Treemap
            cells={treemapCells}
            width={box.width * s.zoom}
            height={box.height * s.zoom}
            formatValue={formatBytes}
            label={`Disk map: ${crumbs.map((c) => c.name).join(", ")}`}
            cellRef={registerCell}
            onHover={setHoverCell}
            onBack={() => {
              if (s.mapPath.length) goToCrumb(s.mapPath.length - 2);
            }}
            onActivate={(id) => {
              const cell = cells.find((c) => c.id === id);
              if (!cell || cell.other) return;
              if (cell.drillable) drillIn({ id: cell.id, name: cell.name });
              else if (cell.itemId) openDrawer({ kind: "item", id: cell.itemId });
            }}
          />
        )}
        {scanning && (
          <div
            className="scan-sweep pointer-events-none absolute inset-y-0 w-1/3"
            aria-hidden="true"
          />
        )}
        {!scanning && cells.length === 0 && (
          <p className="absolute inset-0 grid place-items-center text-muted">
            {folderError ??
              (s.mapView === "folders" ? "Loading the folder map…" : "Nothing to show here.")}
          </p>
        )}
      </div>
      <HoverCard cell={shownCell} item={shownItem} diskTotal={s.volume?.total ?? null} />
    </div>
  );
}
