import { forwardRef } from "react";
import { useStore } from "../../state/store";
import { Settings as Gear } from "lucide-react";
import { AccessBanner } from "./AccessBanner";
import { DiskFullBanner } from "./DiskFullBanner";
import { FilterRow } from "./FilterRow";
import { ModeSwitch } from "./ModeSwitch";
import { ResultPanel } from "./ResultPanel";
import { RulesPanel } from "./RulesPanel";
import { SearchField } from "./SearchField";
import { SelectionBar } from "./SelectionBar";
import { StatusCard } from "./StatusCard";
import { StorageList } from "./StorageList";
import { Tabs } from "./Tabs";
import { isWindows, words } from "../../lib/platform";

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
  const openSettings = useStore((s) => s.openSettings);
  const update = useStore((s) => s.update);
  const showResult = useStore((s) => s.phase === "done" && s.summary !== null);
  return (
    <aside
      className={`jc-sidebar relative flex h-full shrink-0 flex-col bg-surface ${
        compact ? "w-full" : "w-[380px] border-r border-line"
      }`}
    >
      {/* Clears the traffic lights and drags the window. Windows has its own title bar. */}
      <div data-tauri-drag-region className={`${isWindows ? "h-3" : "h-11"} shrink-0`} />
      <div className="flex min-h-0 flex-1 flex-col gap-3.5 px-4">
        <StatusCard />
        <DiskFullBanner />
        <AccessBanner />
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
                <SelectionBar />
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
      <footer className="flex items-center gap-2 border-t border-line px-4 py-3">
        <div className="flex-1">
          <ModeSwitch />
        </div>
        {update && (
          <button
            type="button"
            onClick={() => {
              openSettings("updates");
            }}
            className="h-8 shrink-0 rounded-control bg-accent/15 px-2.5 text-xs font-medium text-accent hover:bg-accent/25"
          >
            Update ready
          </button>
        )}
        <button
          type="button"
          aria-label="Settings"
          title={`Settings (${words.settingsShortcut})`}
          onClick={() => {
            openSettings("general");
          }}
          className="grid size-8 place-items-center rounded-control border border-line bg-raised text-muted hover:text-text"
        >
          <Gear size={15} aria-hidden="true" />
        </button>
      </footer>
    </aside>
  );
});
