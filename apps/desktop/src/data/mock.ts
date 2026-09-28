// Mock scan results for building the UI before the engine is wired in
// (phase 3). Shaped like a real developer's Mac, and deterministic.

import { RULES_BY_ID } from "./catalog";
import type { StorageItem, Volume } from "./types";

export const MOCK_HOME = "/Users/you";
const NOW = Math.floor(Date.now() / 1000);
const DAY = 86_400;
const GB = 1e9;
const MB = 1e6;

export const MOCK_VOLUME: Volume = {
  name: "Macintosh HD",
  total: 494.4 * GB,
  available: 142.3 * GB,
  purgeable: 6.2 * GB,
  used: {
    apps: 58.3 * GB,
    developer: 141.6 * GB,
    system: 71.2 * GB,
    media: 38.4 * GB,
    documents: 29.1 * GB,
    other: 13.5 * GB,
  },
};

/** Small deterministic PRNG so mock data is stable between runs. */
function random(seed: number): () => number {
  let s = seed >>> 0;
  return () => {
    s = (s * 1_664_525 + 1_013_904_223) >>> 0;
    return s / 2 ** 32;
  };
}

interface Seed {
  rule: string;
  path?: string;
  name?: string;
  bytes: number;
  days: number | null;
  project?: { name: string; active: boolean };
  blocked?: string;
  preselected?: boolean;
}

const seeds: Seed[] = [
  {
    rule: "macos.node.npm-cache",
    path: "~/.npm/_cacache",
    bytes: 6.1 * GB,
    days: 40,
    preselected: true,
  },
  {
    rule: "macos.node.yarn-cache",
    path: "~/Library/Caches/Yarn",
    bytes: 1.7 * GB,
    days: 13,
    preselected: true,
  },
  { rule: "macos.node.pnpm-store", path: "~/Library/pnpm/store", bytes: 1.1 * GB, days: 2 },
  {
    rule: "macos.gradle.caches",
    path: "~/.gradle/caches",
    bytes: 927 * MB,
    days: 70,
    preselected: true,
  },
  {
    rule: "macos.rust.cargo-registry",
    path: "~/.cargo/registry/cache",
    bytes: 1.4 * GB,
    days: 5,
    preselected: true,
  },
  {
    rule: "macos.homebrew.cache",
    path: "~/Library/Caches/Homebrew",
    bytes: 2.3 * GB,
    days: 21,
    preselected: true,
  },
  { rule: "macos.maven.repository", path: "~/.m2/repository", bytes: 1.9 * GB, days: 90 },
  {
    rule: "macos.xcode.derived-data",
    name: "MyApp-abxqf",
    bytes: 5.8 * GB,
    days: 12,
    preselected: true,
  },
  {
    rule: "macos.xcode.derived-data",
    name: "Widgets-cdfhp",
    bytes: 2.4 * GB,
    days: 95,
    preselected: true,
  },
  {
    rule: "macos.xcode.derived-data",
    name: "Playground-zzkq",
    bytes: 1.6 * GB,
    days: 220,
    preselected: true,
  },
  { rule: "macos.xcode.device-support", name: "17.2 (21C62)", bytes: 4.1 * GB, days: 300 },
  { rule: "macos.xcode.archives", name: "MyApp 12-03-2025.xcarchive", bytes: 1.4 * GB, days: 400 },
  { rule: "macos.simulator.devices", name: "iPhone 15 Pro · iOS 17.2", bytes: 6.8 * GB, days: 60 },
  { rule: "macos.simulator.devices", name: "iPad Air · iOS 17.0", bytes: 3.4 * GB, days: 190 },
  { rule: "macos.android.emulators", name: "Pixel_8.avd", bytes: 4.5 * GB, days: 120 },
  { rule: "macos.android.system-images", name: "android-34", bytes: 2.5 * GB, days: 120 },
  { rule: "macos.docker.build-cache", bytes: 3.2 * GB, days: null },
  { rule: "macos.docker.unused-images", bytes: 8.4 * GB, days: null },
  { rule: "macos.docker.unused-volumes", bytes: 1.2 * GB, days: null },
  {
    rule: "macos.docker.disk-image",
    path: "~/Library/Containers/com.docker.docker/Data/vms/0/data/Docker.raw",
    bytes: 64 * GB,
    days: 1,
  },
  { rule: "macos.system.app-caches", name: "com.spotify.client", bytes: 819 * MB, days: 0 },
  {
    rule: "macos.system.app-caches",
    name: "com.tinyspeck.slackmacgap",
    bytes: 412 * MB,
    days: 30,
    preselected: true,
  },
  { rule: "macos.system.app-caches", name: "JetBrains", bytes: 949 * MB, days: 1 },
  {
    rule: "macos.system.app-caches",
    name: "com.example.keepme",
    bytes: 120 * MB,
    days: 40,
    blocked: "It's marked to keep with a .jclean-keep file",
  },
  {
    rule: "macos.browsers.chrome",
    path: "~/Library/Caches/Google/Chrome",
    bytes: 792 * MB,
    days: 0,
    preselected: true,
  },
  {
    rule: "macos.browsers.safari",
    path: "~/Library/Caches/com.apple.Safari",
    bytes: 310 * MB,
    days: 0,
    preselected: true,
  },
  { rule: "macos.system.app-logs", name: "JetBrains", bytes: 240 * MB, days: 8, preselected: true },
  {
    rule: "macos.system.crash-reports",
    path: "~/Library/Logs/DiagnosticReports",
    bytes: 38 * MB,
    days: 3,
    preselected: true,
  },
  {
    rule: "macos.system.trash",
    name: "Old renders",
    bytes: 2.1 * GB,
    days: null,
    preselected: true,
  },
  { rule: "macos.downloads.old-installers", name: "Docker.dmg", bytes: 592 * MB, days: 140 },
  { rule: "macos.downloads.old-installers", name: "Xcode_15.xip", bytes: 3.4 * GB, days: 200 },
  {
    rule: "macos.mobile.device-backups",
    name: "Joe's iPhone · March 2025",
    bytes: 1.4 * GB,
    days: 300,
  },
  {
    rule: "macos.media.libraries",
    path: "~/Pictures/Photos Library.photoslibrary",
    bytes: 38.1 * GB,
    days: 2,
  },
  {
    rule: "macos.python.huggingface-models",
    name: "models--meta-llama--Llama-3-8B",
    bytes: 16 * GB,
    days: 150,
  },
  {
    rule: "macos.editors.vscode-caches",
    path: "~/Library/Application Support/Code/CachedData",
    bytes: 640 * MB,
    days: 4,
    preselected: true,
  },
];

const projectNames = [
  "old-site",
  "portfolio",
  "dashboard",
  "admin-portal",
  "mobile-app",
  "landing",
  "api-server",
  "blog",
  "shop-frontend",
  "design-system",
  "analytics",
  "chat-app",
  "notes",
  "invoicing",
  "crm",
  "wiki",
];
const projectRules = [
  { rule: "macos.node.project-dependencies", folder: "node_modules", min: 200 * MB, max: 1.3 * GB },
  { rule: "macos.node.project-caches", folder: ".next", min: 80 * MB, max: 2.5 * GB },
  { rule: "macos.node.project-output", folder: "dist", min: 5 * MB, max: 400 * MB },
  { rule: "macos.rust.project-target", folder: "target", min: 300 * MB, max: 4 * GB },
  { rule: "macos.python.project-environments", folder: ".venv", min: 100 * MB, max: 900 * MB },
];

/** Where items of each rule live, for seeds that only give a name. */
const BASES: Record<string, string> = {
  "macos.xcode.derived-data": "~/Library/Developer/Xcode/DerivedData",
  "macos.xcode.device-support": "~/Library/Developer/Xcode/iOS DeviceSupport",
  "macos.xcode.archives": "~/Library/Developer/Xcode/Archives/2025-03-12",
  "macos.simulator.devices": "~/Library/Developer/CoreSimulator/Devices",
  "macos.android.emulators": "~/.android/avd",
  "macos.android.system-images": "~/Library/Android/sdk/system-images",
  "macos.system.app-caches": "~/Library/Caches",
  "macos.system.app-logs": "~/Library/Logs",
  "macos.system.trash": "~/.Trash",
  "macos.downloads.old-installers": "~/Downloads",
  "macos.mobile.device-backups": "~/Library/Application Support/MobileSync/Backup",
  "macos.python.huggingface-models": "~/.cache/huggingface/hub",
};

function expandHome(path: string): string {
  return path.startsWith("~") ? `${MOCK_HOME}${path.slice(1)}` : path;
}

/** A realistic set of items; `extraProjects` adds more to test long lists. */
export function mockItems(extraProjects = 0): StorageItem[] {
  const items: StorageItem[] = [];
  const add = (seed: Seed, index: number) => {
    const rule = RULES_BY_ID.get(seed.rule);
    if (!rule) return;
    const base = BASES[seed.rule];
    const path = seed.path
      ? expandHome(seed.path)
      : base && seed.name
        ? expandHome(`${base}/${seed.name}`)
        : null;
    const active = seed.project?.active ?? false;
    const risk = seed.project && active && rule.risk === "safe" ? "review" : rule.risk;
    items.push({
      id: `${rule.id}:${path ?? String(index)}:${String(index)}`,
      ruleId: rule.id,
      name: seed.project?.name ?? seed.name ?? null,
      path,
      bytes: Math.round(seed.bytes),
      lastUsed: seed.days === null ? null : NOW - seed.days * DAY,
      risk,
      category: rule.category,
      method: rule.method,
      cleanable: rule.method !== "none" && !seed.blocked,
      blockedReason: seed.blocked ?? null,
      preselected: seed.preselected ?? (risk === "safe" && !!seed.project && !active),
      mayShareBlocks: rule.id === "macos.xcode.derived-data" || rule.id === "macos.node.pnpm-store",
      project: seed.project
        ? { name: seed.project.name, root: `${MOCK_HOME}/code/${seed.project.name}`, active }
        : null,
    });
  };

  seeds.forEach(add);
  const rand = random(42);
  const total = projectNames.length + extraProjects;
  for (let i = 0; i < total; i++) {
    const base = projectNames[i % projectNames.length] ?? "project";
    const name =
      i < projectNames.length ? base : `${base}-${String(Math.floor(i / projectNames.length))}`;
    const days = Math.floor(rand() * 400);
    for (const p of projectRules) {
      if (rand() < 0.45) continue;
      add(
        {
          rule: p.rule,
          path: `~/code/${name}/${p.folder}`,
          bytes: p.min + rand() * (p.max - p.min),
          days,
          project: { name, active: days < 90 },
        },
        items.length,
      );
    }
  }
  return items;
}

/** `?mock=5000` (or VITE_MOCK_ROWS=5000) in dev loads roughly that many rows to check list performance. */
export function mockRowTarget(): number {
  const fromEnv = import.meta.env.DEV
    ? (import.meta.env.VITE_MOCK_ROWS as string | undefined)
    : undefined;
  const param = fromEnv ?? new URLSearchParams(window.location.search).get("mock");
  const n = param ? Number.parseInt(param, 10) : 0;
  return Number.isFinite(n) && n > 0 ? n : 0;
}
