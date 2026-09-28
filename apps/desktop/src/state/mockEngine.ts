// Stands in for the engine until phase 3: streams mock results into the
// store the way real scan events will, and pretends to clean.

import { mockItems, mockRowTarget } from "../data/mock";
import type { PlanDto } from "../bindings";
import type { Method } from "../data/types";
import { useStore } from "./store";

let timer: number | undefined;

function stop() {
  if (timer !== undefined) {
    window.clearInterval(timer);
    timer = undefined;
  }
}

export function startScan() {
  stop();
  const store = useStore.getState();
  store.beginScan("quick");
  const target = mockRowTarget();
  const all = mockItems(target ? Math.ceil(target / 2.75) : 0);
  const batches = 24;
  const perBatch = Math.ceil(all.length / batches);
  let sent = 0;
  // A short pause first, so the skeleton state is visible like on a real scan.
  let tick = -3;
  timer = window.setInterval(() => {
    tick++;
    const s = useStore.getState();
    if (tick <= 0) {
      s.setProgress(0.02 * (tick + 4));
      return;
    }
    s.addItems(all.slice(sent, sent + perBatch));
    sent += perBatch;
    s.setProgress(Math.min(1, sent / all.length));
    if (sent >= all.length) {
      stop();
      s.finishScan({ partial: false, source: "mock" });
    }
  }, 100);
}

/** Stops the scan and keeps what was found so far. */
export function cancelScan() {
  stop();
  useStore.getState().finishScan({ partial: true, source: "mock" });
}

/** A plan for the browser build, shaped like the engine's. */
export function mockPlan(ids: readonly string[]): PlanDto {
  const items = useStore.getState().items.filter((i) => ids.includes(i.id) && i.cleanable);
  const byMethod = new Map<Method, { items: number; bytes: number }>();
  for (const i of items) {
    const m = byMethod.get(i.method) ?? { items: 0, bytes: 0 };
    byMethod.set(i.method, { items: m.items + 1, bytes: m.bytes + i.bytes });
  }
  return {
    totalBytes: items.reduce((n, i) => n + i.bytes, 0),
    items: items.map((i) => ({
      itemId: i.id,
      bytes: i.bytes,
      method: i.method,
      risk: i.risk,
      command: null,
      requiresAdmin: false,
    })),
    skipped: [],
    byMethod: [...byMethod].map(([method, t]) => ({ method, items: t.items, bytes: t.bytes })),
    tools: [],
    needsSecondConfirmation: items.some((i) => i.risk === "caution"),
    runningApps: [],
  };
}

/** Pretends to clean the plan, one item every 80 ms. */
export function runMockClean(plan: PlanDto, label: (id: string) => string) {
  stop();
  const store = useStore.getState();
  store.startCleaning(plan.items.length);
  let i = 0;
  let cleaned = 0;
  let trashed = 0;
  timer = window.setInterval(() => {
    const item = plan.items[i++];
    if (!item) {
      stop();
      useStore.getState().finishCleaning({
        cleanedBytes: cleaned,
        trashedBytes: trashed,
        measuredFreed: cleaned - trashed,
        failed: 0,
        skipped: 0,
      });
      return;
    }
    cleaned += item.bytes;
    if (item.method === "trash") trashed += item.bytes;
    useStore.getState().recordOutcome({
      itemId: item.itemId,
      label: label(item.itemId),
      outcome: "cleaned",
      bytes: item.bytes,
      reason: null,
      method: item.method,
    });
  }, 80);
}
