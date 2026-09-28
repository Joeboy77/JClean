import { Treemap, type TreemapCell } from "@jclean/treemap";
import { animate, useReducedMotion } from "motion/react";
import { useEffect, useLayoutEffect, useRef, useState } from "react";

// A sample Mac's storage (spec §12.2): the same treemap as the app, filling
// in, lighting up what can be freed, then collapsing as the figure counts up.
interface Sample {
  id: string;
  label: string;
  gb: number;
  category: "apps" | "developer" | "system" | "media" | "documents" | "other";
  reclaimable?: boolean;
}

const SAMPLE: Sample[] = [
  { id: "photos", label: "Photos Library", gb: 38.4, category: "media" },
  { id: "xcode", label: "Xcode build data", gb: 21.2, category: "developer", reclaimable: true },
  { id: "apps", label: "Applications", gb: 24.6, category: "apps" },
  { id: "docker", label: "Docker images", gb: 18.0, category: "developer", reclaimable: true },
  { id: "system", label: "System", gb: 19.4, category: "system" },
  {
    id: "node",
    label: "node_modules · 23 projects",
    gb: 12.9,
    category: "developer",
    reclaimable: true,
  },
  { id: "documents", label: "Documents", gb: 14.1, category: "documents" },
  { id: "music", label: "Music", gb: 9.3, category: "media" },
  { id: "npm", label: "npm cache", gb: 6.1, category: "developer", reclaimable: true },
  { id: "caches", label: "App caches", gb: 4.8, category: "apps", reclaimable: true },
  { id: "desktop", label: "Desktop", gb: 5.2, category: "documents" },
  { id: "gradle", label: "Gradle", gb: 3.7, category: "developer", reclaimable: true },
  { id: "installers", label: "Old installers", gb: 2.4, category: "other", reclaimable: true },
  { id: "mail", label: "Mail", gb: 2.2, category: "apps" },
  { id: "logs", label: "Logs", gb: 1.1, category: "system", reclaimable: true },
];
const FREEABLE = SAMPLE.filter((s) => s.reclaimable).reduce((n, s) => n + s.gb, 0);

type Stage = "scanning" | "found" | "cleaned";

function cells(visible: number, stage: Stage): TreemapCell[] {
  // While scanning, the whole map is laid out and cells are revealed in
  // place, so nothing already drawn moves.
  return SAMPLE.filter((s) => !(stage === "cleaned" && s.reclaimable)).map((s, i) => ({
    hidden: i >= visible,
    id: s.id,
    label: s.label,
    value: s.gb,
    color: `var(--cat-${s.category})`,
    emphasis: stage === "found" && s.reclaimable ? 1 : 0,
    highlighted: stage === "found" && s.reclaimable === true,
    ariaLabel: `${s.label}, ${String(s.gb)} gigabytes${s.reclaimable ? ", can be freed" : ""}`,
  }));
}

export function HeroMap() {
  const reduce = useReducedMotion();
  const box = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ width: 0, height: 0 });
  const [visible, setVisible] = useState(reduce ? SAMPLE.length : 0);
  const [stage, setStage] = useState<Stage>(reduce ? "cleaned" : "scanning");
  const [freed, setFreed] = useState(reduce ? FREEABLE : 0);
  const [run, setRun] = useState(0);

  useLayoutEffect(() => {
    const el = box.current;
    if (!el) return;
    const observer = new ResizeObserver(([entry]) => {
      if (entry) setSize({ width: entry.contentRect.width, height: entry.contentRect.height });
    });
    observer.observe(el);
    return () => {
      observer.disconnect();
    };
  }, []);

  useEffect(() => {
    if (reduce) return;
    const timers: number[] = [];
    const at = (ms: number, fn: () => void) => timers.push(window.setTimeout(fn, ms));
    SAMPLE.forEach((_, i) => {
      at(250 + i * 140, () => {
        setVisible(i + 1);
      });
    });
    const found = 250 + SAMPLE.length * 140 + 500;
    at(found, () => {
      setStage("found");
    });
    let stop: (() => void) | undefined;
    at(found + 2200, () => {
      setStage("cleaned");
      const controls = animate(0, FREEABLE, {
        duration: 1.4,
        ease: [0.16, 1, 0.3, 1],
        onUpdate: setFreed,
      });
      stop = () => {
        controls.stop();
      };
    });
    return () => {
      timers.forEach((t) => {
        window.clearTimeout(t);
      });
      stop?.();
    };
  }, [reduce, run]);

  const pill =
    stage === "scanning"
      ? `Scanning · ${String(Math.round((visible / SAMPLE.length) * 100))}%`
      : stage === "found"
        ? `Ready · ${FREEABLE.toFixed(1)} GB can be freed`
        : `Freed ${freed.toFixed(1)} GB`;

  return (
    <figure
      className="relative overflow-hidden rounded-card border border-line bg-surface p-3 shadow-2xl"
      aria-label="A sample Mac's storage in JClean"
    >
      <div className="mb-3 flex items-center justify-between gap-3 px-1">
        <span className="flex gap-1.5" aria-hidden="true">
          <span className="size-2.5 rounded-full bg-[#ee6b7e]/70" />
          <span className="size-2.5 rounded-full bg-[#edb548]/70" />
          <span className="size-2.5 rounded-full bg-[#4ccb9f]/70" />
        </span>
        <span
          className="tabular rounded-full border border-line bg-bg px-3 py-1 text-xs text-text"
          role="status"
          aria-live="off"
        >
          <span
            className={`mr-2 inline-block size-1.5 rounded-full align-middle ${stage === "scanning" ? "animate-pulse bg-accent" : "bg-safe"}`}
          />
          {pill}
        </span>
        {stage === "cleaned" && !reduce ? (
          <button
            type="button"
            onClick={() => {
              setVisible(0);
              setStage("scanning");
              setFreed(0);
              setRun((r) => r + 1);
            }}
            className="text-xs text-muted hover:text-text"
          >
            Replay
          </button>
        ) : (
          <span className="w-10" />
        )}
      </div>
      <div ref={box} className="relative aspect-[16/10] w-full overflow-hidden rounded-control">
        {size.width > 0 && (
          <Treemap
            cells={cells(visible, stage)}
            width={size.width}
            height={size.height}
            gap={3}
            label="Sample disk map"
            formatValue={(v) => `${v.toFixed(1)} GB`}
          />
        )}
        {stage === "scanning" && (
          <div
            className="scan-sweep pointer-events-none absolute inset-y-0 w-1/3"
            aria-hidden="true"
          />
        )}
      </div>
    </figure>
  );
}
