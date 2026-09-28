import { AnimatePresence, motion } from "motion/react";
import type { MapCellView, StorageItem } from "../../data/types";
import { RISK_COPY } from "../../data/riskCopy";
import { formatAgo, formatBytes } from "../../lib/format";
import { fade } from "../../lib/motion";
import { RiskIcon } from "../ui/RiskBadge";

interface HoverCardProps {
  cell: MapCellView | null;
  item: StorageItem | null;
  diskTotal: number | null;
}

/** Details of the hovered cell, bottom right of the canvas (spec §5.2). */
export function HoverCard({ cell, item, diskTotal }: HoverCardProps) {
  return (
    <AnimatePresence>
      {cell && (
        <motion.div
          key="card"
          role="status"
          initial={{ opacity: 0, y: 6 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: 6 }}
          transition={fade}
          className="pointer-events-none absolute right-5 bottom-5 z-20 w-64 rounded-card border border-line bg-surface/95 p-3.5 shadow-2xl backdrop-blur"
        >
          <p className="truncate font-medium text-text">{cell.name}</p>
          <p className="tabular mt-1 text-lg font-semibold text-text">{formatBytes(cell.bytes)}</p>
          <dl className="mt-2 space-y-1 text-xs">
            {diskTotal !== null && diskTotal > 0 && (
              <div className="flex justify-between gap-3">
                <dt className="text-muted">Share of disk</dt>
                <dd className="tabular text-text">
                  {((cell.bytes / diskTotal) * 100).toFixed(1)}%
                </dd>
              </div>
            )}
            {cell.reclaimable > 0 && (
              <div className="flex justify-between gap-3">
                <dt className="text-muted">Can be freed</dt>
                <dd className="tabular text-safe">{formatBytes(cell.reclaimable)}</dd>
              </div>
            )}
            {item?.lastUsed != null && (
              <div className="flex justify-between gap-3">
                <dt className="text-muted">Last used</dt>
                <dd className="text-text">{formatAgo(item.lastUsed)}</dd>
              </div>
            )}
            {item && (
              <div className="flex justify-between gap-3">
                <dt className="text-muted">Risk</dt>
                <dd className="flex items-center gap-1 text-text">
                  <RiskIcon risk={item.risk} size={11} />
                  {RISK_COPY[item.risk].title}
                </dd>
              </div>
            )}
          </dl>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
