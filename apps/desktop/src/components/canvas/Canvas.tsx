import { Minus, Plus } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { Profiler, useMemo } from "react";
import { onRender } from "../../dev/profile";
import { formatBytes } from "../../lib/format";
import { fade } from "../../lib/motion";
import { startScan } from "../../state/engine";
import { useStore } from "../../state/store";
import { LogoMark } from "../LogoMark";
import { MapArea } from "./MapArea";

const ZOOMS = [1, 1.5, 2, 3];

function StatePill() {
  const phase = useStore((s) => s.phase);
  const progress = useStore((s) => s.progress);
  const stage = useStore((s) => s.stage);
  const items = useStore((s) => s.items);
  const freed = useStore((s) => s.freed);
  const reclaimable = useMemo(
    () => items.filter((i) => i.cleanable).reduce((n, i) => n + i.bytes, 0),
    [items],
  );

  const text = {
    idle: "Not scanned",
    scanning: `${stage || "Scanning"} · ${String(Math.round(progress * 100))}%`,
    results: `Ready · ${formatBytes(reclaimable)} can be freed`,
    cleaning: "Cleaning…",
    done: `Freed ${formatBytes(freed)}`,
  }[phase];
  const dot = {
    idle: "bg-muted",
    scanning: "bg-accent",
    results: "bg-safe",
    cleaning: "bg-accent",
    done: "bg-safe",
  }[phase];

  return (
    <div
      role="status"
      className="tabular flex items-center gap-2 rounded-b-card border border-t-0 border-line bg-surface px-4 py-2 text-sm text-text shadow-lg"
    >
      <span
        className={`size-2 rounded-full ${dot} ${phase === "scanning" || phase === "cleaning" ? "animate-pulse" : ""}`}
      />
      <AnimatePresence mode="wait" initial={false}>
        <motion.span
          key={phase}
          initial={{ opacity: 0, y: 4 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: -4 }}
          transition={fade}
        >
          {text}
        </motion.span>
      </AnimatePresence>
    </div>
  );
}

function ZoomControl() {
  const zoom = useStore((s) => s.zoom);
  const setZoom = useStore((s) => s.setZoom);
  const phase = useStore((s) => s.phase);
  const i = ZOOMS.indexOf(zoom);
  const off = phase === "idle";
  return (
    <div
      className="flex items-center rounded-control border border-line bg-surface"
      role="group"
      aria-label="Zoom"
    >
      <button
        type="button"
        disabled={off || i <= 0}
        onClick={() => {
          setZoom(ZOOMS[i - 1] ?? 1);
        }}
        aria-label="Zoom out"
        className="grid size-7 place-items-center text-muted hover:text-text disabled:opacity-40"
      >
        <Minus size={14} aria-hidden="true" />
      </button>
      <span className="tabular w-9 text-center text-xs text-muted">{Math.round(zoom * 100)}%</span>
      <button
        type="button"
        disabled={off || i >= ZOOMS.length - 1}
        onClick={() => {
          setZoom(ZOOMS[i + 1] ?? zoom);
        }}
        aria-label="Zoom in"
        className="grid size-7 place-items-center text-muted hover:text-text disabled:opacity-40"
      >
        <Plus size={14} aria-hidden="true" />
      </button>
    </div>
  );
}

export function Canvas() {
  const phase = useStore((s) => s.phase);
  return (
    <main className="relative flex h-full min-w-0 flex-1 flex-col bg-bg" aria-label="Disk map">
      <div
        data-tauri-drag-region
        className="relative flex h-14 shrink-0 items-start justify-center"
      >
        <StatePill />
        <div className="absolute top-3 right-5 flex items-center gap-4">
          <ZoomControl />
          <span className="flex items-center gap-2 text-accent">
            <LogoMark size={18} />
            <span className="font-semibold text-text">JClean</span>
          </span>
        </div>
      </div>

      <div className="relative min-h-0 flex-1 px-6 pt-2 pb-6">
        <AnimatePresence mode="wait" initial={false}>
          {phase === "idle" ? (
            <motion.div
              key="empty"
              className="flex h-full flex-col items-center justify-center gap-5"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={fade}
            >
              <svg width="200" height="120" viewBox="0 0 200 120" fill="none" aria-hidden="true">
                <rect
                  x="4"
                  y="18"
                  width="192"
                  height="84"
                  rx="16"
                  stroke="var(--line)"
                  strokeWidth="2"
                />
                <rect x="20" y="34" width="120" height="52" rx="6" fill="var(--raised)" />
                <rect x="146" y="34" width="34" height="24" rx="6" fill="var(--raised)" />
                <rect
                  x="146"
                  y="62"
                  width="34"
                  height="24"
                  rx="6"
                  stroke="var(--accent)"
                  strokeWidth="2"
                  strokeDasharray="5 4"
                />
              </svg>
              <div className="text-center">
                <p className="text-lg font-semibold text-text">See what's filling your Mac</p>
                <p className="mt-1 text-muted">
                  JClean only reads file sizes and dates. Nothing leaves your Mac.
                </p>
              </div>
              <button
                type="button"
                onClick={() => {
                  startScan("quick");
                }}
                className="h-9 rounded-control bg-accent-strong px-5 font-medium text-white hover:brightness-110"
              >
                Scan this Mac
              </button>
            </motion.div>
          ) : (
            <motion.div
              key="map"
              className="relative h-full"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={fade}
            >
              {import.meta.env.DEV && import.meta.env.VITE_DEMO === "fps" ? (
                <Profiler id="map" onRender={onRender}>
                  <MapArea />
                </Profiler>
              ) : (
                <MapArea />
              )}
            </motion.div>
          )}
        </AnimatePresence>
      </div>
    </main>
  );
}
