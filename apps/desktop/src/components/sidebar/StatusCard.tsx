import { motion } from "motion/react";
import { useMemo } from "react";
import { CATEGORIES, type Category, type StorageItem, type Volume } from "../../data/types";
import { formatBytes } from "../../lib/format";
import { spring } from "../../lib/motion";
import { cancelScan, startClean, startScan } from "../../state/mockEngine";
import { selectedBytes } from "../../state/selectors";
import { useStore } from "../../state/store";
import { AnimatedBytes } from "../ui/AnimatedBytes";

const CATEGORY_LABEL: Record<Category, string> = {
  apps: "Apps",
  developer: "Developer",
  system: "System",
  media: "Media",
  documents: "Documents",
  other: "Other",
};

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

function CapacityBar({
  volume,
  items,
  showReclaimable,
}: {
  volume: Volume;
  items: readonly StorageItem[];
  showReclaimable: boolean;
}) {
  const reclaimable = useMemo(() => reclaimableByCategory(items), [items]);
  const pct = (bytes: number) => `${String((bytes / volume.total) * 100)}%`;
  const used = volume.total - volume.available;
  // Scale category totals so they fill exactly the used part of the bar.
  const known = CATEGORIES.reduce((n, c) => n + volume.used[c], 0);
  const scale = known > 0 ? used / known : 0;

  return (
    <div>
      <div
        className="flex h-2.5 overflow-hidden rounded-full bg-cat-free"
        role="img"
        aria-label={`${formatBytes(used)} used of ${formatBytes(volume.total)}`}
      >
        {CATEGORIES.map((c) => {
          const bytes = volume.used[c] * scale;
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
      </div>
      <ul className="mt-2.5 flex flex-wrap gap-x-3 gap-y-1 text-xs text-muted">
        {CATEGORIES.map((c) => (
          <li key={c} className="flex items-center gap-1.5">
            <span className="size-2 rounded-full" style={{ background: `var(--cat-${c})` }} />
            {CATEGORY_LABEL[c]}
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
  const freed = useStore((s) => s.freed);
  const chosen = useMemo(() => selectedBytes(items, selected), [items, selected]);

  const usedRatio = (volume.total - volume.available) / volume.total;
  const percent = Math.round(progress * 100);

  const primary = {
    idle: { label: "Scan", onClick: startScan, disabled: false, aria: undefined },
    scanning: {
      label: `Scanning… ${String(percent)}%`,
      onClick: cancelScan,
      disabled: false,
      aria: "Cancel scan",
    },
    results: {
      label: chosen > 0 ? `Clean ${formatBytes(chosen)}` : "Nothing selected",
      onClick: startClean,
      disabled: chosen === 0,
      aria: undefined,
    },
    cleaning: { label: "Cleaning…", onClick: () => undefined, disabled: true, aria: undefined },
    done: { label: "Scan again", onClick: startScan, disabled: false, aria: undefined },
  }[phase];

  const secondary = {
    idle: { label: "Full scan", disabled: false },
    scanning: { label: "Full scan", disabled: true },
    results: { label: "Scan again", disabled: false },
    cleaning: { label: "Scan again", disabled: true },
    done: { label: "Full scan", disabled: false },
  }[phase];

  return (
    <section
      aria-label="Storage overview"
      className="rounded-card border border-line p-4"
      style={{
        background: `linear-gradient(140deg, ${healthTint(usedRatio)}, transparent 70%), var(--bg)`,
      }}
    >
      <p className="text-muted">{volume.name}</p>
      <p className="mt-0.5 flex items-baseline gap-1.5">
        <AnimatedBytes bytes={volume.available} className="text-xl font-semibold" />
        <span className="text-muted">
          free of <span className="tabular">{formatBytes(volume.total)}</span>
        </span>
      </p>

      <div className="mt-3">
        <CapacityBar volume={volume} items={items} showReclaimable={phase === "results"} />
      </div>

      <div className="mt-4 flex gap-2">
        <motion.button
          type="button"
          layout
          transition={spring}
          onClick={primary.onClick}
          disabled={primary.disabled}
          {...(primary.aria ? { "aria-label": primary.aria } : {})}
          className="tabular relative h-9 flex-1 overflow-hidden rounded-control bg-accent px-4 font-medium text-white transition-[filter] hover:brightness-110 disabled:cursor-default disabled:opacity-50 disabled:hover:brightness-100"
        >
          {phase === "scanning" && (
            <span
              aria-hidden="true"
              className="absolute inset-y-0 left-0 bg-white/15 transition-[width] duration-200"
              style={{ width: `${String(percent)}%` }}
            />
          )}
          <span className="relative">{primary.label}</span>
        </motion.button>
        <button
          type="button"
          onClick={startScan}
          disabled={secondary.disabled}
          className="h-9 rounded-control border border-line bg-raised px-4 text-text transition-colors hover:border-muted disabled:cursor-default disabled:opacity-50 disabled:hover:border-line"
        >
          {secondary.label}
        </button>
      </div>

      {phase === "done" && freed > 0 && (
        <p className="mt-3 text-muted" role="status">
          <AnimatedBytes bytes={freed} prefix="Cleaned " className="text-safe" />. Your Mac has more
          room now.
        </p>
      )}
      {phase === "results" && partial && (
        <p className="mt-3 text-muted" role="status">
          Scan stopped early, so these results are partial.
        </p>
      )}
    </section>
  );
}
