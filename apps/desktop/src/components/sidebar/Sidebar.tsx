import { forwardRef } from "react";
import { useStore } from "../../state/store";
import { FilterRow } from "./FilterRow";
import { ModeSwitch } from "./ModeSwitch";
import { ResultPanel } from "./ResultPanel";
import { RulesPanel } from "./RulesPanel";
import { SearchField } from "./SearchField";
import { StatusCard } from "./StatusCard";
import { StorageList } from "./StorageList";
import { Tabs } from "./Tabs";

interface SidebarProps {
  compact: boolean;
  onToggleLayout: () => void;
}

/** The control column (spec §5.2). Fixed at 380 px beside the canvas; fills
 * the window in compact mode. It never moves when the canvas slides. */
export const Sidebar = forwardRef<HTMLInputElement, SidebarProps>(function Sidebar(
  { compact, onToggleLayout },
  searchRef,
) {
  const tab = useStore((s) => s.tab);
  const showResult = useStore((s) => s.phase === "done" && s.summary !== null);
  return (
    <aside
      className={`relative flex h-full shrink-0 flex-col bg-surface ${
        compact ? "w-full" : "w-[380px] border-r border-line"
      }`}
    >
      {/* Clears the traffic lights and drags the window. */}
      <div data-tauri-drag-region className="h-11 shrink-0" />
      <div className="flex min-h-0 flex-1 flex-col gap-3.5 px-4">
        <StatusCard />
        {showResult ? (
          <ResultPanel />
        ) : (
          <>
            <Tabs />
            {tab === "categories" ? (
              <div
                id="panel-categories"
                role="tabpanel"
                aria-labelledby="tab-categories"
                className="flex min-h-0 flex-1 flex-col gap-3"
              >
                <FilterRow compact={compact} onToggleLayout={onToggleLayout} />
                <SearchField ref={searchRef} />
                <StorageList />
              </div>
            ) : (
              <div
                id="panel-rules"
                role="tabpanel"
                aria-labelledby="tab-rules"
                className="flex min-h-0 flex-1 flex-col"
              >
                <RulesPanel />
              </div>
            )}
          </>
        )}
      </div>
      <footer className="border-t border-line px-4 py-3">
        <ModeSwitch />
      </footer>
    </aside>
  );
});
