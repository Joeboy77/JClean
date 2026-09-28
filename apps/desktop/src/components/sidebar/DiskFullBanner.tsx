import { HardDrive } from "lucide-react";
import { useMemo } from "react";
import { formatBytes } from "../../lib/format";
import { useStore } from "../../state/store";

/** "Disk nearly full" (spec §5.5): over 90% used. */
export function DiskFullBanner() {
  const volume = useStore((s) => s.volume);
  const items = useStore((s) => s.items);
  const phase = useStore((s) => s.phase);
  const reclaimable = useMemo(
    () => items.filter((i) => i.cleanable && i.risk === "safe").reduce((n, i) => n + i.bytes, 0),
    [items],
  );
  if (!volume || phase === "cleaning" || phase === "done") return null;
  const used = (volume.total - volume.available) / volume.total;
  if (used < 0.9) return null;
  return (
    <div
      className="flex items-start gap-3 rounded-card border border-caution/40 bg-caution/10 p-3"
      role="status"
    >
      <HardDrive size={16} className="mt-0.5 shrink-0 text-caution" aria-hidden="true" />
      <div className="min-w-0">
        <p className="text-text">
          Your disk is almost full. {formatBytes(volume.available)} is free.
        </p>
        <p className="mt-0.5 text-xs text-muted">
          {reclaimable > 0
            ? `${formatBytes(reclaimable)} below is safe to clean.`
            : "macOS and your apps slow down when space runs out. Scan to see what can go."}
        </p>
      </div>
    </div>
  );
}
