// Talks to the Rust engine over IPC (spec §3). In a plain browser (UI work
// without Tauri) it falls back to the mock engine.

import { Channel, isTauri } from "@tauri-apps/api/core";
import { commands, type ScanUpdate } from "../bindings";
import { MOCK_HOME, MOCK_VOLUME } from "../data/mock";
import type { MapCellView, StorageItem } from "../data/types";
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
