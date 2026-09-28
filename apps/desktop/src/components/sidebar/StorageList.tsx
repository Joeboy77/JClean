import { useVirtualizer } from "@tanstack/react-virtual";
import { useMemo, useRef, useState, type KeyboardEvent } from "react";
import { useShallow } from "zustand/react/shallow";
import { RULES_BY_ID } from "../../data/catalog";
import { formatBytes } from "../../lib/format";
import { buildRows, type GroupRow, type ItemRow, type Row } from "../../state/selectors";
import { useStore } from "../../state/store";
import { GroupRowView, ItemRowView, SectionHeader, SkeletonRowView } from "./ListRows";
import { ROW_HEIGHT } from "./rowHeight";

const SKELETON: Row[] = Array.from({ length: 8 }, (_, i) => ({
  kind: "skeleton",
  key: `skeleton:${String(i)}`,
}));

function EmptyState({ title, body }: { title: string; body: string }) {
  return (
    <div
      className="flex flex-1 flex-col items-center justify-center gap-1 px-8 text-center"
      role="status"
    >
      <p className="text-md text-text">{title}</p>
      <p className="text-muted">{body}</p>
    </div>
  );
}

/** The grouped, virtualized storage list (spec §5.2). One tab stop: arrow
 * keys move between rows, Space toggles, Enter opens details. */
export function StorageList() {
  const s = useStore(
    useShallow((st) => ({
      phase: st.phase,
      items: st.items,
      selected: st.selected,
      expanded: st.expanded,
      collapsed: st.collapsed,
      filters: st.filters,
      search: st.search,
      audience: st.audience,
      disabledRules: st.disabledRules,
      volume: st.volume,
    })),
  );
  const actions = useStore(
    useShallow((st) => ({
      setSelected: st.setSelected,
      toggleExpanded: st.toggleExpanded,
      toggleSection: st.toggleSection,
      openDrawer: st.openDrawer,
    })),
  );

  const rows = useMemo<Row[]>(() => {
    if (s.phase === "scanning" && s.items.length === 0) return SKELETON;
    return buildRows({ ...s, rules: RULES_BY_ID });
  }, [s]);

  const scrollRef = useRef<HTMLDivElement>(null);
  const [active, setActive] = useState(0);
  const current = Math.min(active, Math.max(0, rows.length - 1));

  // The React Compiler isn't used, so TanStack's non-memoizable API is fine here.
  // eslint-disable-next-line react-hooks/incompatible-library
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: (i) => ROW_HEIGHT[rows[i]?.kind ?? "group"],
    getItemKey: (i) => rows[i]?.key ?? i,
    overscan: 10,
  });

  const toggleGroup = (row: GroupRow) => {
    const ids = row.items.filter((i) => i.cleanable).map((i) => i.id);
    actions.setSelected(ids, row.check !== "all");
  };
  const toggleItem = (row: ItemRow) => {
    if (row.item.cleanable) actions.setSelected([row.item.id], !row.checked);
  };
  const openGroup = (row: GroupRow) => {
    actions.openDrawer({ kind: "group", risk: row.risk, ruleId: row.rule.id });
  };
  const openItem = (row: ItemRow) => {
    actions.openDrawer({ kind: "item", id: row.item.id });
  };

  const moveTo = (index: number) => {
    const next = Math.max(0, Math.min(rows.length - 1, index));
    setActive(next);
    virtualizer.scrollToIndex(next, { align: "auto" });
  };

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const row = rows[current];
    if (!row) return;
    const handled = (() => {
      switch (e.key) {
        case "ArrowDown":
          moveTo(current + 1);
          return true;
        case "ArrowUp":
          moveTo(current - 1);
          return true;
        case "Home":
          moveTo(0);
          return true;
        case "End":
          moveTo(rows.length - 1);
          return true;
        case " ":
          if (row.kind === "group") toggleGroup(row);
          else if (row.kind === "item") toggleItem(row);
          else if (row.kind === "section") actions.toggleSection(row.risk);
          return true;
        case "Enter":
          if (row.kind === "group") openGroup(row);
          else if (row.kind === "item") openItem(row);
          else if (row.kind === "section") actions.toggleSection(row.risk);
          return true;
        case "ArrowRight":
          if (row.kind === "section" && row.collapsed) actions.toggleSection(row.risk);
          else if (row.kind === "group" && row.expandable && !row.expanded)
            actions.toggleExpanded(row.key);
          else moveTo(current + 1);
          return true;
        case "ArrowLeft":
          if (row.kind === "section" && !row.collapsed) actions.toggleSection(row.risk);
          else if (row.kind === "group" && row.expanded) actions.toggleExpanded(row.key);
          else if (row.kind === "item") {
            // Up to the item's group.
            for (let i = current - 1; i >= 0; i--) {
              if (rows[i]?.kind === "group") {
                moveTo(i);
                break;
              }
            }
          }
          return true;
        default:
          return false;
      }
    })();
    if (handled) e.preventDefault();
  };

  if (s.phase === "idle") {
    return <EmptyState title="Not scanned yet" body="Scan to see where your storage is going." />;
  }
  if (rows.length === 0) {
    if (s.search.trim())
      return <EmptyState title="No matches" body={`Nothing matches “${s.search.trim()}”.`} />;
    return (
      <EmptyState
        title="Your Mac is tidy"
        body={`Nothing to clean right now. ${formatBytes(s.volume.available)} free.`}
      />
    );
  }

  const skeleton = rows[0]?.kind === "skeleton";
  return (
    <div
      ref={scrollRef}
      role="tree"
      aria-label="Storage"
      aria-busy={skeleton}
      tabIndex={0}
      {...(!skeleton ? { "aria-activedescendant": `row-${String(current)}` } : {})}
      onKeyDown={onKeyDown}
      className="group/list scroll-area -mx-2 min-h-0 flex-1 overflow-y-auto px-2 pb-3 outline-none"
    >
      <div className="relative w-full" style={{ height: virtualizer.getTotalSize() }}>
        {virtualizer.getVirtualItems().map((v) => {
          const row = rows[v.index];
          if (!row) return null;
          const isActive = v.index === current && !skeleton;
          return (
            <div
              key={v.key}
              id={`row-${String(v.index)}`}
              role="treeitem"
              aria-level={row.kind === "item" ? 3 : row.kind === "group" ? 2 : 1}
              aria-selected={isActive}
              {...(row.kind === "group" && row.expandable ? { "aria-expanded": row.expanded } : {})}
              {...(row.kind === "section" ? { "aria-expanded": !row.collapsed } : {})}
              data-active={isActive}
              onMouseDown={() => {
                setActive(v.index);
              }}
              className="absolute inset-x-0 top-0 py-0.5"
              style={{ height: v.size, transform: `translateY(${String(v.start)}px)` }}
            >
              {row.kind === "section" && (
                <SectionHeader
                  row={row}
                  active={isActive}
                  onToggle={() => {
                    actions.toggleSection(row.risk);
                  }}
                />
              )}
              {row.kind === "group" && (
                <GroupRowView
                  row={row}
                  active={isActive}
                  audience={s.audience}
                  onToggleCheck={() => {
                    toggleGroup(row);
                  }}
                  onToggleExpand={() => {
                    actions.toggleExpanded(row.key);
                  }}
                  onOpen={() => {
                    openGroup(row);
                  }}
                />
              )}
              {row.kind === "item" && (
                <ItemRowView
                  row={row}
                  active={isActive}
                  audience={s.audience}
                  onToggleCheck={() => {
                    toggleItem(row);
                  }}
                  onOpen={() => {
                    openItem(row);
                  }}
                />
              )}
              {row.kind === "skeleton" && <SkeletonRowView />}
            </div>
          );
        })}
      </div>
    </div>
  );
}
