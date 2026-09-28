import { CircleCheck } from "lucide-react";
import { motion } from "motion/react";
import { useState } from "react";
import { formatBytes } from "../../lib/format";
import { spring } from "../../lib/motion";
import { emptyTrash, reviewClean } from "../../state/engine";
import { useStore } from "../../state/store";
import { AnimatedBytes } from "../ui/AnimatedBytes";
import { RiskIcon } from "../ui/RiskBadge";

/** After a clean (spec §5.4): what was freed, the Trash reminder, and
 * anything that couldn't be removed, with a retry (spec §5.5). */
export function ResultPanel() {
  const summary = useStore((s) => s.summary);
  const outcomes = useStore((s) => s.outcomes);
  const dismiss = useStore((s) => s.dismissResult);
  const [trash, setTrash] = useState<{
    state: "idle" | "working" | "done";
    freed: number;
    note: string | null;
  }>({
    state: "idle",
    freed: 0,
    note: null,
  });
  if (!summary) return null;

  const problems = outcomes.filter((o) => o.outcome !== "cleaned");
  const freedNow = summary.cleanedBytes - summary.trashedBytes;
  const inTrash = trash.state === "done" ? 0 : summary.trashedBytes;

  return (
    <motion.section
      aria-label="Cleaning result"
      className="scroll-area -mx-2 min-h-0 flex-1 space-y-4 overflow-y-auto px-2 pb-3"
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      transition={spring}
    >
      <div className="rounded-card border border-line bg-bg p-4" role="status">
        <p className="flex items-center gap-2 text-muted">
          <CircleCheck size={16} className="text-safe" aria-hidden="true" />
          {problems.length ? "Done, with a few exceptions" : "All done"}
        </p>
        <p className="mt-1 text-2xl font-semibold text-text">
          <AnimatedBytes bytes={freedNow + trash.freed} prefix="Cleaned " />
        </p>
        {summary.measuredFreed !== null && summary.measuredFreed > 0 && (
          <p className="mt-1 text-xs text-muted">
            Your disk's free space went up by {formatBytes(summary.measuredFreed)}. macOS can take a
            moment to report the rest.
          </p>
        )}
      </div>

      {inTrash > 0 && (
        <div className="rounded-card border border-line p-4">
          <p className="text-text">
            {formatBytes(inTrash)} is in the Trash. Empty the Trash to free it.
          </p>
          <p className="mt-1 text-xs text-muted">
            Until then you can still put those items back from the Trash.
          </p>
          <button
            type="button"
            disabled={trash.state === "working"}
            onClick={() => {
              setTrash({ state: "working", freed: 0, note: null });
              void emptyTrash().then((r) => {
                setTrash({ state: "done", freed: r.freed, note: r.note });
              });
            }}
            className="mt-3 h-8 rounded-control border border-line bg-raised px-3 text-text hover:border-muted disabled:opacity-50"
          >
            {trash.state === "working" ? "Emptying…" : "Empty Trash"}
          </button>
        </div>
      )}
      {trash.note && <p className="text-xs text-muted">{trash.note}</p>}

      {problems.length > 0 && (
        <div className="rounded-card border border-line p-4">
          <p className="text-text">
            {problems.length} {problems.length === 1 ? "item couldn't" : "items couldn't"} be
            removed
          </p>
          <ul className="mt-2 space-y-2">
            {problems.slice(0, 12).map((p) => (
              <li key={p.itemId} className="flex gap-2 text-xs">
                <RiskIcon risk={p.outcome === "failed" ? "caution" : "review"} size={12} />
                <span className="min-w-0">
                  <span className="block truncate text-text">{p.label}</span>
                  <span className="text-muted">{p.reason}</span>
                </span>
              </li>
            ))}
            {problems.length > 12 && (
              <li className="text-xs text-muted">and {problems.length - 12} more</li>
            )}
          </ul>
          <button
            type="button"
            onClick={() => {
              void reviewClean(problems.map((p) => p.itemId));
            }}
            className="mt-3 h-8 rounded-control border border-line bg-raised px-3 text-text hover:border-muted"
          >
            Try again
          </button>
        </div>
      )}

      <button
        type="button"
        onClick={dismiss}
        className="h-9 w-full rounded-control bg-accent-strong font-medium text-white hover:brightness-110"
      >
        Done
      </button>
    </motion.section>
  );
}
