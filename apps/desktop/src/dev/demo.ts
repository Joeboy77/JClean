// Dev-only scenarios for reviewing UI states without clicking:
//   VITE_DEMO=scan|full|drawer|review|clean|compact|expanded|fps VITE_MOCK_ROWS=5000 pnpm tauri dev
//   JCLEAN_DEV_ROOT=/tmp/jc VITE_DEMO=clean pnpm tauri dev   (clean a fixture from `jclean-cli fixture /tmp/jc`)
// Never included in release builds (guarded by import.meta.env.DEV).

import { isTauri } from "@tauri-apps/api/core";
import { commands } from "../bindings";
import { confirmClean, reviewClean, startScan } from "../state/engine";
import { useStore } from "../state/store";

export function runDemo(scenario: string) {
  startScan(scenario === "full" ? "full" : "quick");
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
  if (scenario === "review" || scenario === "clean") {
    whenDone(() => {
      void reviewClean().then(() => {
        const plan = useStore.getState().plan;
        if (scenario === "clean" && plan)
          setTimeout(() => {
            confirmClean(plan);
          }, 3000);
      });
    });
  }
  if (scenario === "fps") {
    whenDone(() => {
      void measureFps();
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

const wait = (ms: number) =>
  new Promise((resolve) => {
    setTimeout(resolve, ms);
  });

/** Times every React update of the map while exercising it (spec §5.8:
 * 60 fps). Frame rate itself can't be measured reliably when macOS
 * throttles a background or Low Power Mode window, but work per update can:
 * under 16 ms per update leaves room for 60 fps. */
async function measureFps() {
  const { commits } = await import("./profile");
  const s = () => useStore.getState();
  const section = async (name: string, run: () => Promise<void>) => {
    commits.length = 0;
    await run();
    const ms = commits.map((c) => c.ms).sort((a, b) => a - b);
    const p95 = ms[Math.floor(ms.length * 0.95)] ?? 0;
    return `${name}: ${String(ms.length)} updates, p95 ${p95.toFixed(1)} ms, worst ${(ms.at(-1) ?? 0).toFixed(1)} ms`;
  };

  const counts = new Map<string, number>();
  for (const i of s().items) counts.set(i.ruleId, (counts.get(i.ruleId) ?? 0) + 1);
  const busiest = [...counts].sort((a, b) => b[1] - a[1])[0]?.[0] ?? "";
  const lines: string[] = [];

  lines.push(
    await section(
      `drill into ${busiest} (${String(counts.get(busiest) ?? 0)} items, capped at 150 cells)`,
      async () => {
        s().drillIn({ id: `rule:${busiest}`, name: busiest });
        await wait(900);
      },
    ),
  );
  const inside = s()
    .items.filter((i) => i.ruleId === busiest)
    .slice(0, 60);
  lines.push(
    await section("hover 60 rows (row → cell link)", async () => {
      for (const item of inside) {
        s().setHover({ rowKey: `item:${item.id}`, itemIds: [item.id] });
        await wait(30);
      }
      s().setHover(null);
    }),
  );
  lines.push(
    await section("zoom 150% → 200% → 100%", async () => {
      for (const zoom of [1.5, 2, 1]) {
        s().setZoom(zoom);
        await wait(400);
      }
    }),
  );
  lines.push(
    await section("back to the top level", async () => {
      s().goToCrumb(-1);
      await wait(900);
    }),
  );
  const report = lines.join(" | ");
  if (isTauri()) await commands.devLog(report);
  else console.info(report);
}
