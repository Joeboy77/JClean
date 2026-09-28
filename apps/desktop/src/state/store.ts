import { create } from "zustand";
import { MOCK_VOLUME } from "../data/mock";
import type { Audience, Risk, ScanPhase, StorageItem, Volume } from "../data/types";
import { NO_FILTERS, type FilterKey, type Filters } from "./selectors";

export type Tab = "categories" | "rules";

export type DrawerTarget =
  { kind: "group"; risk: Risk; ruleId: string } | { kind: "item"; id: string };

interface State {
  phase: ScanPhase;
  /** 0–1 while scanning or cleaning. */
  progress: number;
  /** The last scan was cancelled; results are partial. */
  partial: boolean;
  items: StorageItem[];
  selected: ReadonlySet<string>;
  expanded: ReadonlySet<string>;
  collapsed: ReadonlySet<Risk>;
  filters: Filters;
  search: string;
  tab: Tab;
  audience: Audience;
  drawer: DrawerTarget | null;
  volume: Volume;
  /** Bytes freed by the last clean. */
  freed: number;
  disabledRules: ReadonlySet<string>;
}

interface Actions {
  beginScan: () => void;
  addItems: (items: StorageItem[]) => void;
  setProgress: (progress: number) => void;
  finishScan: (partial: boolean) => void;
  beginClean: () => void;
  finishClean: (cleanedIds: readonly string[], freed: number) => void;
  setSelected: (ids: readonly string[], on: boolean) => void;
  toggleExpanded: (key: string) => void;
  toggleSection: (risk: Risk) => void;
  toggleFilter: (key: FilterKey) => void;
  setSearch: (search: string) => void;
  setTab: (tab: Tab) => void;
  setAudience: (audience: Audience) => void;
  openDrawer: (target: DrawerTarget) => void;
  closeDrawer: () => void;
  toggleRule: (ruleId: string) => void;
}

function toggled<T>(set: ReadonlySet<T>, value: T): Set<T> {
  const next = new Set(set);
  if (next.has(value)) next.delete(value);
  else next.add(value);
  return next;
}

export const useStore = create<State & Actions>()((set) => ({
  phase: "idle",
  progress: 0,
  partial: false,
  items: [],
  selected: new Set(),
  expanded: new Set(),
  // "For your information" starts collapsed (spec §5.2).
  collapsed: new Set<Risk>(["info"]),
  filters: NO_FILTERS,
  search: "",
  tab: "categories",
  audience: "developer",
  drawer: null,
  volume: MOCK_VOLUME,
  freed: 0,
  disabledRules: new Set(),

  beginScan: () => {
    set({
      phase: "scanning",
      progress: 0,
      partial: false,
      items: [],
      selected: new Set(),
      drawer: null,
    });
  },
  addItems: (items) => {
    set((s) => {
      // Safe, unused items arrive pre-selected (spec §9, step 1).
      const selected = new Set(s.selected);
      for (const item of items) if (item.preselected && item.cleanable) selected.add(item.id);
      return { items: [...s.items, ...items], selected };
    });
  },
  setProgress: (progress) => {
    set({ progress });
  },
  finishScan: (partial) => {
    set({ phase: "results", progress: 1, partial });
  },
  beginClean: () => {
    set({ phase: "cleaning", progress: 0, drawer: null });
  },
  finishClean: (cleanedIds, freed) => {
    set((s) => {
      const gone = new Set(cleanedIds);
      return {
        phase: "done",
        progress: 1,
        freed,
        items: s.items.filter((i) => !gone.has(i.id)),
        selected: new Set([...s.selected].filter((id) => !gone.has(id))),
        volume: { ...s.volume, available: s.volume.available + freed },
      };
    });
  },
  setSelected: (ids, on) => {
    set((s) => {
      const selected = new Set(s.selected);
      for (const id of ids) {
        if (on) selected.add(id);
        else selected.delete(id);
      }
      return { selected };
    });
  },
  toggleExpanded: (key) => {
    set((s) => ({ expanded: toggled(s.expanded, key) }));
  },
  toggleSection: (risk) => {
    set((s) => ({ collapsed: toggled(s.collapsed, risk) }));
  },
  toggleFilter: (key) => {
    set((s) => ({ filters: { ...s.filters, [key]: !s.filters[key] } }));
  },
  setSearch: (search) => {
    set({ search });
  },
  setTab: (tab) => {
    set({ tab });
  },
  setAudience: (audience) => {
    set({ audience });
  },
  openDrawer: (target) => {
    set({ drawer: target });
  },
  closeDrawer: () => {
    set({ drawer: null });
  },
  toggleRule: (ruleId) => {
    set((s) => ({ disabledRules: toggled(s.disabledRules, ruleId) }));
  },
}));
