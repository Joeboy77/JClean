import { motion } from "motion/react";
import { useMemo } from "react";
import { CATEGORIES, type Category, type StorageItem, type Volume } from "../../data/types";
import { formatAgo, formatBytes } from "../../lib/format";
import { spring } from "../../lib/motion";
import { cancelScan, reviewClean, startScan } from "../../state/engine";
import { CATEGORY_NAME, usageByCategory } from "../../state/mapModel";
import { selectedBytes } from "../../state/selectors";
import { useStore } from "../../state/store";
import { AnimatedBytes } from "../ui/AnimatedBytes";

/** Card tint follows disk health (spec §5.2): violet, amber from 80%, rose from 90%. */
function healthTint(usedRatio: number): string {
  if (usedRatio >= 0.9) return "rgba(238, 107, 126, 0.16)";
  if (usedRatio >= 0.8) return "rgba(237, 181, 72, 0.14)";
  return "rgba(139, 108, 255, 0.16)";
}

function reclaimableByCategory(items: readonly StorageItem[]): Record<Category, number> {
  const out = { apps: 0, developer: 0, system: 0, media: 0, documents: 0, other: 0 };
  for (const item of items) if (item.cleanable) out[item.category] += item.bytes;
  return out;
}

interface BarProps {
  volume: Volume;
  items: readonly StorageItem[];
  showReclaimable: boolean;
  filling: boolean;
}

function CapacityBar({ volume, items, showReclaimable, filling }: BarProps) {
  const used = volume.total - volume.available;
  const usage = useMemo(() => usageByCategory(used, items), [used, items]);
  const reclaimable = useMemo(() => reclaimableByCategory(items), [items]);
  const pct = (bytes: number) => `${String((bytes / volume.total) * 100)}%`;

  return (
    <div>
      <div
        className="relative flex h-2.5 overflow-hidden rounded-full bg-cat-free"
        role="img"
        aria-label={`${formatBytes(used)} used of ${formatBytes(volume.total)}`}
      >
        {CATEGORIES.map((c) => {
          const bytes = usage[c];
          const share = bytes > 0 ? Math.min(1, reclaimable[c] / bytes) : 0;
          return (
            <motion.div
              key={c}
              layout
              transition={spring}
              className="relative h-full border-r border-bg/60 last:border-r-0"
              style={{ width: pct(bytes), background: `var(--cat-${c})`, opacity: 0.55 }}
            >
              {showReclaimable && share > 0 && (
                <span
                  className="reclaimable absolute inset-y-0 right-0"
                  style={{ width: `${String(share * 100)}%` }}
                />
              )}
            </motion.div>
          );
        })}
        {/* While scanning, segments fill in left to right (spec §5.8). */}
        {filling && <span className="bar-fill absolute inset-0 bg-cat-free" aria-hidden="true" />}
      </div>
      <ul className="mt-2.5 flex flex-wrap gap-x-3 gap-y-1 text-xs text-muted">
        {CATEGORIES.map((c) => (
          <li key={c} className="flex items-center gap-1.5">
            <span className="size-2 rounded-full" style={{ background: `var(--cat-${c})` }} />
            {CATEGORY_NAME[c]}
          </li>
        ))}
        <li className="flex items-center gap-1.5">
          <span className="size-2 rounded-full border border-line bg-cat-free" />
          Free
        </li>
      </ul>
    </div>
  );
}

export function StatusCard() {
  const phase = useStore((s) => s.phase);
  const progress = useStore((s) => s.progress);
  const partial = useStore((s) => s.partial);
  const volume = useStore((s) => s.volume);
  const items = useStore((s) => s.items);
  const selected = useStore((s) => s.selected);
  const source = useStore((s) => s.source);
  const cachedAt = useStore((s) => s.cachedAt);
  const error = useStore((s) => s.error);
  const planning = useStore((s) => s.planning);
  const chosen = useMemo(() => selectedBytes(items, selected), [items, selected]);

  const usedRatio = volume ? (volume.total - volume.available) / volume.total : 0;
  const percent = Math.round(progress * 100);

  const primary = {
    idle: {
      label: "Scan",
      onClick: () => {
        startScan("quick");
      },
      disabled: false,
      aria: undefined,
    },
    scanning: {
      label: `Scanning… ${String(percent)}%`,
      onClick: cancelScan,
      disabled: false,
      aria: "Cancel scan",
    },
    results: {
      label: planning
        ? "Preparing…"
        : chosen > 0
          ? `Clean ${formatBytes(chosen)}`
          : "Nothing selected",
      onClick: () => {
        void reviewClean();
      },
      disabled: chosen === 0 || planning,
      aria: undefined,
    },
    cleaning: {
      label: `Cleaning… ${String(percent)}%`,
      onClick: () => undefined,
      disabled: true,
      aria: undefined,
    },
    done: {
      label: "Scan again",
      onClick: () => {
        startScan("quick");
      },
      disabled: false,
      aria: undefined,
    },
  }[phase];

  const secondary = {
    idle: { label: "Full scan", mode: "full" as const, disabled: false },
    scanning: { label: "Full scan", mode: "full" as const, disabled: true },
    results: { label: "Full scan", mode: "full" as const, disabled: false },
    cleaning: { label: "Full scan", mode: "full" as const, disabled: true },
    done: { label: "Full scan", mode: "full" as const, disabled: false },
  }[phase];

  const primaryButton = (
    <motion.button
      type="button"
      layout
      transition={spring}
      onClick={primary.onClick}
      disabled={primary.disabled}
      {...(primary.aria ? { "aria-label": primary.aria } : {})}
      className="tabular relative h-9 w-full overflow-hidden rounded-control bg-accent px-4 font-medium text-white transition-[filter] hover:brightness-110 disabled:cursor-default disabled:opacity-50 disabled:hover:brightness-100"
    >
      {(phase === "scanning" || phase === "cleaning") && (
        <span
          aria-hidden="true"
          className="absolute inset-y-0 left-0 bg-white/15 transition-[width] duration-200"
          style={{ width: `${String(percent)}%` }}
        />
      )}
      <span className="relative">{primary.label}</span>
    </motion.button>
  );

  return (
    <section
      aria-label="Storage overview"
      className="rounded-card border border-line p-4"
      style={{
        background: `linear-gradient(140deg, ${healthTint(usedRatio)}, transparent 70%), var(--bg)`,
      }}
    >
      {volume ? (
        <>
          <p className="text-muted">{volume.name}</p>
          <p className="mt-0.5 flex items-baseline gap-1.5">
            <AnimatedBytes bytes={volume.available} className="text-xl font-semibold" />
            <span className="text-muted">
              free of <span className="tabular">{formatBytes(volume.total)}</span>
            </span>
          </p>
          <div className="mt-3">
            <CapacityBar
              volume={volume}
              items={items}
              showReclaimable={phase === "results"}
              filling={phase === "scanning"}
            />
          </div>
        </>
      ) : (
        <div aria-hidden="true" className="space-y-2.5">
          <span className="skeleton block h-3.5 w-24 rounded-full" />
          <span className="skeleton block h-7 w-48 rounded-full" />
          <span className="skeleton block h-2.5 w-full rounded-full" />
          <span className="skeleton block h-3 w-40 rounded-full" />
        </div>
      )}

      <div className="mt-4 flex gap-2">
        <div className="flex-1">{primaryButton}</div>
        <button
          type="button"
          onClick={() => {
            startScan(secondary.mode);
          }}
          disabled={secondary.disabled}
          className="h-9 rounded-control border border-line bg-raised px-4 text-text transition-colors hover:border-muted disabled:cursor-default disabled:opacity-50 disabled:hover:border-line"
        >
          {secondary.label}
        </button>
      </div>

      {volume && volume.purgeable > 1e9 && phase !== "scanning" && (
        <p className="mt-3 text-xs text-muted">
          Includes {formatBytes(volume.purgeable)} macOS can free on its own when it needs space.
        </p>
      )}
      {source === "cached" && cachedAt !== null && phase === "results" && (
        <p className="mt-3 text-xs text-muted" role="status">
          From your last scan, {formatAgo(cachedAt)}.
        </p>
      )}
      {phase === "results" && partial && (
        <p className="mt-3 text-muted" role="status">
          Scan stopped early, so these results are partial.
        </p>
      )}
      {error && (
        <p className="mt-3 text-caution" role="alert">
          {error}
        </p>
      )}
    </section>
  );
}
