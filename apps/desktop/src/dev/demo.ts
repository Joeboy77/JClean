// Dev-only scenarios for reviewing UI states without clicking:
//   VITE_DEMO=scan|drawer|compact VITE_MOCK_ROWS=5000 pnpm tauri dev
// Never included in release builds (guarded by import.meta.env.DEV).

import { isTauri } from "@tauri-apps/api/core";
import { commands } from "../bindings";
import { startScan } from "../state/mockEngine";
import { useStore } from "../state/store";

export function runDemo(scenario: string) {
  startScan();
  const whenDone = (fn: () => void) => {
    const unsub = useStore.subscribe((s) => {
      if (s.phase === "results") {
        unsub();
        setTimeout(fn, 400);
      }
    });
  };
  if (scenario === "drawer") {
    whenDone(() => {
      const s = useStore.getState();
      s.toggleExpanded("group:safe:macos.xcode.derived-data");
      s.openDrawer({ kind: "group", risk: "safe", ruleId: "macos.xcode.derived-data" });
    });
  }
  if (scenario === "compact" && isTauri()) {
    whenDone(() => {
      useStore.getState().toggleExpanded("group:safe:macos.node.project-dependencies");
      void commands.setWindowMode("compact");
    });
  }
  if (scenario === "expanded" && isTauri()) {
    whenDone(() => {
      useStore.getState().toggleExpanded("group:safe:macos.node.project-dependencies");
      void commands.setWindowMode("expanded");
    });
  }
}
