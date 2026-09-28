import { useMemo } from "react";
import { useShallow } from "zustand/react/shallow";
import { formatBytes } from "../../lib/format";
import { reviewClean, visibleSelection } from "../../state/engine";
import { visibleItems } from "../../state/selectors";
import { useStore } from "../../state/store";

/** What's selected, with quick ways to change it and clean just that. */
export function SelectionBar() {
  const s = useStore(
    useShallow((st) => ({
      items: st.items,
      rules: st.rules,
      audience: st.audience,
      filters: st.filters,
      search: st.search,
      selected: st.selected,
      expanded: st.expanded,
      collapsed: st.collapsed,
      disabledRules: st.disabledRules,
      phase: st.phase,
      planning: st.planning,
      setSelected: st.setSelected,
    })),
  );
  const shown = useMemo(() => visibleItems(s).filter((i) => i.cleanable), [s]);
  // Everything selected in this mode, even if a search or filter hides it
  // now: the same set the main Clean button cleans.
  const chosen = useMemo(() => {
    const ids = new Set(visibleSelection());
    return s.items.filter((i) => ids.has(i.id));
    // eslint-disable-next-line react-hooks/exhaustive-deps -- visibleSelection reads these from the store
  }, [s.items, s.selected, s.audience, s.disabledRules, s.rules]);
  const bytes = chosen.reduce((n, i) => n + i.bytes, 0);
  if (s.phase !== "results" || shown.length === 0) return null;

  const allShown = shown.every((i) => s.selected.has(i.id));
  return (
    <div className="flex items-center gap-2 text-xs" role="toolbar" aria-label="Selection">
      <span className="tabular min-w-0 flex-1 truncate text-muted" aria-live="polite">
        {chosen.length === 0
          ? "Nothing selected"
          : `${String(chosen.length)} selected · ${formatBytes(bytes)}`}
      </span>
      {!allShown && (
        <button
          type="button"
          onClick={() => {
            s.setSelected(
              shown.map((i) => i.id),
              true,
            );
          }}
          className="rounded-row px-2 py-1 text-accent hover:bg-raised"
        >
          Select all
        </button>
      )}
      {chosen.length > 0 && (
        <>
          <button
            type="button"
            onClick={() => {
              s.setSelected(
                chosen.map((i) => i.id),
                false,
              );
            }}
            className="rounded-row px-2 py-1 text-muted hover:bg-raised hover:text-text"
          >
            Clear
          </button>
          <button
            type="button"
            disabled={s.planning}
            onClick={() => {
              void reviewClean(chosen.map((i) => i.id));
            }}
            className="tabular rounded-row bg-accent px-2.5 py-1 font-medium text-white hover:brightness-110 disabled:opacity-50"
          >
            Clean {formatBytes(bytes)}
          </button>
        </>
      )}
    </div>
  );
}
