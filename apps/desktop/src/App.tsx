import { useEffect, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { commands, type AppInfo } from "./bindings";
import { LogoMark } from "./components/LogoMark";

export function App() {
  const [info, setInfo] = useState<AppInfo | null>(null);

  useEffect(() => {
    if (isTauri()) {
      void commands.appInfo().then(setInfo);
    }
  }, []);

  return (
    <div className="flex h-full">
      {/* Sidebar (380 px, spec §5.1). The top strip clears the traffic lights and drags the window. */}
      <aside className="flex w-[380px] shrink-0 flex-col border-r border-line bg-surface">
        <div data-tauri-drag-region className="h-13 shrink-0" />
        <div className="flex flex-1 flex-col gap-6 px-5 pb-5">
          <div className="flex items-center gap-2.5 text-accent">
            <LogoMark size={22} />
            <span className="text-lg font-semibold text-text">JClean</span>
          </div>
          <div className="rounded-card border border-line bg-bg p-4">
            <p className="text-muted">Storage overview</p>
            <p className="tabular mt-1 text-xl font-semibold">Not scanned yet</p>
          </div>
        </div>
      </aside>

      {/* Canvas: the disk map arrives in phase 3. Hidden in compact widths. */}
      <main className="relative hidden flex-1 flex-col min-[760px]:flex">
        <div data-tauri-drag-region className="h-13 shrink-0" />
        <div className="flex flex-1 flex-col items-center justify-center gap-4 text-accent">
          <LogoMark size={56} />
          <p className="text-md text-muted">See what's filling your Mac, and clear it safely.</p>
        </div>
        {info && (
          <p className="tabular absolute right-5 bottom-4 text-xs text-muted">
            Version {info.version}
          </p>
        )}
      </main>
    </div>
  );
}
