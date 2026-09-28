// Talks to the Rust engine over IPC (spec §3). In a plain browser (UI work
// without Tauri) it falls back to the mock engine.

import { Channel, isTauri } from "@tauri-apps/api/core";
import { commands, type CleanUpdate, type PlanDto, type ScanUpdate } from "../bindings";
import { RULES_BY_ID } from "../data/catalog";
import { MOCK_HOME, MOCK_VOLUME } from "../data/mock";
import type { MapCellView, StorageItem } from "../data/types";
import { itemName, ruleLabel, visibleIn } from "./selectors";
import * as mock from "./mockEngine";
import { useStore, type ScanMode } from "./store";

export const isLive = isTauri();

/** Loads the volume, the last scan (shown instantly), then runs a quick scan (spec §4.1, §10). */
export async function init(options: { autoScan: boolean }) {
  const store = useStore.getState();
  if (!isLive) {
    store.setVolume(MOCK_VOLUME);
    store.setHome(MOCK_HOME);
    return;
  }
  const [info, volume, cached] = await Promise.all([
    commands.appInfo(),
    commands.volumeInfo(),
    commands.cachedScan(),
  ]);
  store.setHome(info.home);
  if (volume) store.setVolume(volume);
  if (cached) store.showCached(cached.items, cached.savedAt);
  if (options.autoScan) startScan("quick");
}

export async function refreshVolume() {
  if (!isLive) return;
  const volume = await commands.volumeInfo();
  if (volume) useStore.getState().setVolume(volume);
}

export function startScan(mode: ScanMode) {
  if (!isLive) {
    mock.startScan();
    return;
  }
  const store = useStore.getState();
  store.beginScan(mode);

  // Items arrive one by one; add them to the store once per frame.
  let pending: StorageItem[] = [];
  let scheduled = false;
  const flush = () => {
    scheduled = false;
    if (pending.length) {
      useStore.getState().addItems(pending);
      pending = [];
    }
  };

  const channel = new Channel<ScanUpdate>();
  channel.onmessage = (update) => {
    const s = useStore.getState();
    switch (update.kind) {
      case "stage":
        s.setStage(update.label);
        break;
      case "progress":
        s.setProgress(update.fraction);
        break;
      case "item":
        pending.push(update.item);
        if (!scheduled) {
          scheduled = true;
          requestAnimationFrame(flush);
        }
        break;
      case "finished":
        flush();
        s.finishScan({ partial: update.partial, notes: update.notes, hasTree: update.hasTree });
        void refreshVolume();
        break;
    }
  };

  void commands.startScan(mode, channel).then((r) => {
    if (r.status === "error") useStore.getState().failScan(r.error);
  });
}

export function cancelScan() {
  if (!isLive) {
    mock.cancelScan();
    return;
  }
  void commands.cancelScan();
}

/** One level of the full scan's folder map. */
export async function folderLevel(id: string): Promise<MapCellView[]> {
  if (!isLive) return [];
  const r = await commands.mapLevel(id);
  if (r.status === "error") throw new Error(r.error);
  return r.data.map((c) => ({
    id: c.id,
    name: c.name,
    bytes: c.bytes,
    category: c.category,
    reclaimable: c.reclaimable,
    itemId: c.itemId,
    drillable: c.hasChildren,
    other: c.other,
  }));
}

/** The list label for an item, for naming it after it's gone. */
function labelFor(id: string): string {
  const s = useStore.getState();
  const item = s.items.find((i) => i.id === id);
  const rule = item ? RULES_BY_ID.get(item.ruleId) : undefined;
  if (!item || !rule) return id;
  const name = itemName(item);
  const base = ruleLabel(rule, s.audience);
  return name && name !== base ? `${base} · ${name}` : base;
}

/** Selected items the user can currently see: Everyday mode never cleans
 * something it doesn't show. */
export function visibleSelection(): string[] {
  const s = useStore.getState();
  return s.items
    .filter((i) => {
      const rule = RULES_BY_ID.get(i.ruleId);
      return (
        s.selected.has(i.id) &&
        rule !== undefined &&
        !s.disabledRules.has(rule.id) &&
        visibleIn(rule, s.audience)
      );
    })
    .map((i) => i.id);
}

/** Builds the plan and opens the confirmation sheet (spec §9, 2–3). */
export async function reviewClean(ids: readonly string[] = visibleSelection()) {
  const store = useStore.getState();
  store.setPlanning(true);
  if (!isLive) {
    store.showPlan(mock.mockPlan(ids));
    return;
  }
  const r = await commands.planClean([...ids]);
  if (r.status === "error") {
    store.setPlanning(false);
    store.failScan(r.error);
    return;
  }
  store.showPlan(r.data);
}

export function cancelReview() {
  useStore.getState().showPlan(null);
}

/** Runs the plan the user confirmed (spec §9, 4–6). */
export function confirmClean(plan: PlanDto) {
  if (!isLive) {
    mock.runMockClean(plan, labelFor);
    return;
  }
  const labels = new Map(plan.items.map((i) => [i.itemId, labelFor(i.itemId)]));
  useStore.getState().startCleaning(plan.items.length);
  const channel = new Channel<CleanUpdate>();
  channel.onmessage = (u) => {
    const s = useStore.getState();
    if (u.kind === "item") {
      s.recordOutcome({
        itemId: u.itemId,
        label: labels.get(u.itemId) ?? u.itemId,
        outcome: u.outcome === "cleaned" || u.outcome === "failed" ? u.outcome : "skipped",
        bytes: u.bytes,
        reason: u.reason,
        method: u.method,
      });
    } else if (u.kind === "finished") {
      s.finishCleaning({
        cleanedBytes: u.cleanedBytes,
        trashedBytes: u.trashedBytes,
        measuredFreed: u.measuredFreed,
        failed: u.failed,
        skipped: u.skipped,
      });
      void refreshVolume();
    }
  };
  void commands.runClean(channel).then((r) => {
    if (r.status === "error") {
      useStore.getState().failScan(r.error);
      useStore.getState().dismissResult();
    }
  });
}

/** Empties the Trash after a clean moved things there. */
export async function emptyTrash(): Promise<{ freed: number; note: string | null }> {
  if (!isLive) return { freed: useStore.getState().summary?.trashedBytes ?? 0, note: null };
  const r = await commands.emptyTrash();
  if (r.status === "error") return { freed: 0, note: r.error };
  void refreshVolume();
  return { freed: r.data.freed, note: r.data.note };
}
