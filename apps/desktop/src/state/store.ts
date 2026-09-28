import { create } from "zustand";
import type { Audience, MapView, Risk, ScanPhase, StorageItem, Volume } from "../data/types";
import { NO_FILTERS, type FilterKey, type Filters } from "./selectors";

export type Tab = "categories" | "rules";
export type ScanMode = "quick" | "full";

export type DrawerTarget =
  { kind: "group"; risk: Risk; ruleId: string } | { kind: "item"; id: string };

/** Where the results on screen came from. */
export type Source = "none" | "live" | "cached" | "mock";

export interface Crumb {
  id: string;
  name: string;
}

/** A hovered list row, for lighting up its map cell (spec §5.2). */
export interface Hover {
  rowKey: string;
  itemIds: readonly string[];
}

interface State {
  phase: ScanPhase;
  /** 0–1 while scanning or cleaning. */
  progress: number;
  stage: string;
  scanMode: ScanMode;
  /** The last scan was cancelled; results are partial. */
  partial: boolean;
  source: Source;
  /** Unix seconds, when results come from the cache. */
  cachedAt: number | null;
  notes: string[];
  error: string | null;
  items: StorageItem[];
  selected: ReadonlySet<string>;
  expanded: ReadonlySet<string>;
  collapsed: ReadonlySet<Risk>;
  filters: Filters;
  search: string;
  tab: Tab;
  audience: Audience;
  drawer: DrawerTarget | null;
  volume: Volume | null;
  home: string;
  /** Bytes freed by the last clean. */
  freed: number;
  disabledRules: ReadonlySet<string>;
  /** The last scan has a folder tree (a full scan). */
  hasTree: boolean;
  mapView: MapView;
  mapPath: readonly Crumb[];
  zoom: number;
  hover: Hover | null;
  /** The map cell a hovered row points at, for the connector line. */
  linkCell: string | null;
}

interface Actions {
  beginScan: (mode: ScanMode) => void;
  addItems: (items: StorageItem[]) => void;
  setProgress: (progress: number) => void;
  setStage: (stage: string) => void;
  finishScan: (result: {
    partial: boolean;
    notes?: string[];
    hasTree?: boolean;
    source?: Source;
  }) => void;
  failScan: (message: string) => void;
  showCached: (items: StorageItem[], savedAt: number) => void;
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
  setVolume: (volume: Volume) => void;
  setHome: (home: string) => void;
  setMapView: (view: MapView) => void;
  drillIn: (crumb: Crumb) => void;
  goToCrumb: (index: number) => void;
  setZoom: (zoom: number) => void;
  setHover: (hover: Hover | null) => void;
  setLinkCell: (id: string | null) => void;
}

function toggled<T>(set: ReadonlySet<T>, value: T): Set<T> {
  const next = new Set(set);
  if (next.has(value)) next.delete(value);
  else next.add(value);
  return next;
}

function preselect(items: readonly StorageItem[], into: Set<string>): Set<string> {
  // Safe, unused items arrive pre-selected (spec §9, step 1).
  for (const item of items) if (item.preselected && item.cleanable) into.add(item.id);
  return into;
}

export const useStore = create<State & Actions>()((set) => ({
  phase: "idle",
  progress: 0,
  stage: "",
  scanMode: "quick",
  partial: false,
  source: "none",
  cachedAt: null,
  notes: [],
  error: null,
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
  volume: null,
  home: "",
  freed: 0,
  disabledRules: new Set(),
  hasTree: false,
  mapView: "found",
  mapPath: [],
  zoom: 1,
  hover: null,
  linkCell: null,

  beginScan: (mode) => {
    set({
      phase: "scanning",
      scanMode: mode,
      progress: 0,
      stage: "Starting",
      partial: false,
      error: null,
      notes: [],
      items: [],
      selected: new Set(),
      drawer: null,
      hover: null,
      mapPath: [],
      mapView: "found",
      hasTree: false,
      cachedAt: null,
    });
  },
  addItems: (items) => {
    set((s) => ({
      items: [...s.items, ...items],
      selected: preselect(items, new Set(s.selected)),
    }));
  },
  setProgress: (progress) => {
    set({ progress });
  },
  setStage: (stage) => {
    set({ stage });
  },
  finishScan: ({ partial, notes = [], hasTree = false, source = "live" }) => {
    set({
      phase: "results",
      progress: 1,
      partial,
      notes,
      hasTree,
      source,
      mapView: hasTree ? "folders" : "found",
      mapPath: [],
    });
  },
  failScan: (message) => {
    set((s) => ({ phase: s.items.length ? "results" : "idle", error: message, partial: true }));
  },
  showCached: (items, savedAt) => {
    set({
      phase: "results",
      source: "cached",
      cachedAt: savedAt,
      items,
      selected: preselect(items, new Set()),
      progress: 1,
      mapPath: [],
      mapView: "found",
    });
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
        volume: s.volume ? { ...s.volume, available: s.volume.available + freed } : null,
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
    set({ audience, mapPath: [] });
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
  setVolume: (volume) => {
    set({ volume });
  },
  setHome: (home) => {
    set({ home });
  },
  setMapView: (mapView) => {
    set({ mapView, mapPath: [] });
  },
  drillIn: (crumb) => {
    set((s) => ({ mapPath: [...s.mapPath, crumb], hover: null }));
  },
  goToCrumb: (index) => {
    // -1 is the top level.
    set((s) => ({ mapPath: s.mapPath.slice(0, index + 1), hover: null }));
  },
  setZoom: (zoom) => {
    set({ zoom });
  },
  setHover: (hover) => {
    set({ hover });
  },
  setLinkCell: (linkCell) => {
    set((s) => (s.linkCell === linkCell ? s : { linkCell }));
  },
}));
