// Talks to the Rust engine over IPC (spec §3). In a plain browser (UI work
// without Tauri) it falls back to the mock engine.

import { Channel, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  commands,
  type CleanUpdate,
  type Link,
  type PlanDto,
  type ScanUpdate,
  type Settings,
} from "../bindings";
import { MOCK_HOME, MOCK_VOLUME } from "../data/mock";
import type { Audience, MapCellView, StorageItem } from "../data/types";
import { itemName, ruleLabel, visibleIn } from "./selectors";
import * as mock from "./mockEngine";
import { useStore, type ScanMode } from "./store";

export const isLive = isTauri();

/** Loads settings, the volume and the last scan (shown instantly), then
 * runs a quick scan (spec §4.1, §10). On first launch, onboarding runs
 * instead and starts the first scan itself (spec §5.9). */
export async function init(options: { autoScan: boolean }) {
  const store = useStore.getState();
  if (!isLive) {
    store.setVolume(MOCK_VOLUME);
    store.setHome(MOCK_HOME);
    return;
  }
  const [info, volume, settings, rules, fda, cached] = await Promise.all([
    commands.appInfo(),
    commands.volumeInfo(),
    commands.getSettings(),
    commands.getRules(),
    commands.fullDiskAccess(),
    commands.cachedScan(),
  ]);
  store.setHome(info.home);
  store.setRules(rules);
  store.setSettings(settings);
  store.setFullDiskAccess(fda);
  if (volume) store.setVolume(volume);
  void listen("tray-quick-scan", () => {
    startScan("quick");
  });
  if (!settings.onboarded) return;
  if (cached) {
    store.showCached(cached.items, cached.savedAt);
    store.setNeedsAccess(cached.needsAccess);
  }
  if (options.autoScan && settings.scanOnLaunch) startScan("quick");
}

/** Saves a change to Settings and applies it (rules, menu bar, launch at login). */
export async function updateSettings(patch: Partial<Settings>): Promise<string | null> {
  const store = useStore.getState();
  const current = store.settings;
  if (!current) return null;
  const next: Settings = { ...current, ...patch };
  store.setSettings(next);
  if (!isLive) return null;
  const r = await commands.saveSettings(next);
  if (r.status === "error") {
    store.setSettings(current);
    return r.error;
  }
  store.setSettings(r.data);
  store.setRules(await commands.getRules());
  return null;
}

/** Everyday or Developer (spec §2): changes rules and labels, never safety. */
export function setMode(mode: Audience) {
  const store = useStore.getState();
  store.setAudience(mode);
  void updateSettings({ mode });
}

/** Switches a rule on or off from the Rules tab or Settings. */
export function toggleRuleEnabled(ruleId: string) {
  const store = useStore.getState();
  store.toggleRule(ruleId);
  void updateSettings({ disabledRules: [...useStore.getState().disabledRules] });
}

/** Result of a Settings action that can fail with a message for the user. */
async function settingsCall(
  call: Promise<{ status: "ok"; data: Settings } | { status: "error"; error: string }>,
) {
  const r = await call;
  if (r.status === "error") return r.error;
  useStore.getState().setSettings(r.data);
  useStore.getState().setRules(await commands.getRules());
  return null;
}

export async function pickFolder(): Promise<string | null> {
  return isLive ? commands.pickFolder() : null;
}

export function addCustomFolder(path: string, name: string, risk: "safe" | "review" | "caution") {
  return settingsCall(commands.addCustomFolder(path, name, risk));
}

export async function importRulePack(): Promise<string | null> {
  if (!isLive) return null;
  const picked = await commands.pickRulePack();
  if (picked.status === "error") return picked.error;
  if (!picked.data) return null;
  return settingsCall(commands.addRulePack(picked.data.name, picked.data.text));
}

export function removeCustomRule(ruleId: string) {
  return settingsCall(commands.removeCustomRule(ruleId));
}

export async function checkFullDiskAccess(): Promise<boolean> {
  if (!isLive) return true;
  const granted = await commands.fullDiskAccess();
  useStore.getState().setFullDiskAccess(granted);
  return granted;
}

export function openLink(link: Link) {
  if (isLive) void commands.openLink(link);
}

export function revealItem(itemId: string) {
  if (isLive) void commands.revealItem(itemId);
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
        s.setNeedsAccess(update.needsAccess);
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
  const rule = item ? s.rules.get(item.ruleId) : undefined;
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
      const rule = s.rules.get(i.ruleId);
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
  // With "Ask before cleaning" off, go straight ahead unless something needs
  // a person's attention: caution items always ask (spec §5.11).
  const settings = useStore.getState().settings;
  const plan = r.data;
  if (
    settings &&
    !settings.confirmBeforeCleaning &&
    !plan.needsSecondConfirmation &&
    plan.runningApps.length === 0 &&
    plan.items.length > 0
  ) {
    store.setPlanning(false);
    confirmClean(plan);
    return;
  }
  store.showPlan(plan);
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
