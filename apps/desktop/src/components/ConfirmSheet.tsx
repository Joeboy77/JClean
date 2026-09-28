import { AnimatePresence, motion } from "motion/react";
import { useEffect, useMemo, useRef, useState } from "react";
import type { PlanDto } from "../bindings";
import { RISK_COPY } from "../data/riskCopy";
import type { Method } from "../data/types";
import { formatBytes } from "../lib/format";
import { fade, spring } from "../lib/motion";
import { cancelReview, confirmClean, reviewClean } from "../state/engine";
import { itemName, ruleLabel } from "../state/selectors";
import { useStore } from "../state/store";
import { RiskIcon } from "./ui/RiskBadge";

const METHOD_LINE: Record<Method, string> = {
  delete: "Deleted permanently",
  trash: "Moved to the Trash",
  command: "Cleared by their own tools",
  none: "Not cleaned",
};

/** Tool names as people know them. */
const TOOL_NAMES: Record<string, string> = {
  brew: "Homebrew",
  docker: "Docker",
  npm: "npm",
  pnpm: "pnpm",
  go: "Go",
  xcrun: "Xcode",
  conda: "Conda",
  uv: "uv",
  dart: "Dart",
  tmutil: "Time Machine",
};

/** "Clean 38.6 GB?" (spec §5.4), with a second step for caution items (spec §7.3). */
export function ConfirmSheet() {
  const plan = useStore((s) => s.plan);
  return <AnimatePresence>{plan && <Sheet key="sheet" plan={plan} />}</AnimatePresence>;
}

function Sheet({ plan }: { plan: PlanDto }) {
  const items = useStore((s) => s.items);
  const audience = useStore((s) => s.audience);
  const rules = useStore((s) => s.rules);
  const [step, setStep] = useState<"review" | "caution">("review");
  const primaryRef = useRef<HTMLButtonElement>(null);

  const byId = useMemo(() => new Map(items.map((i) => [i.id, i])), [items]);
  const label = (id: string) => {
    const item = byId.get(id);
    const rule = item ? rules.get(item.ruleId) : undefined;
    if (!item || !rule) return id;
    const name = itemName(item);
    const base = ruleLabel(rule, audience);
    return name && name !== base ? `${base} · ${name}` : base;
  };

  const caution = plan.items.filter((i) => i.risk === "caution");
  const review = plan.items.filter((i) => i.risk === "review");
  const admin = plan.items.filter((i) => i.requiresAdmin);
  const total = formatBytes(plan.totalBytes);

  useEffect(() => {
    primaryRef.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") cancelReview();
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
    };
  }, [step]);

  const go = () => {
    if (step === "review" && plan.needsSecondConfirmation) setStep("caution");
    else confirmClean(plan);
  };

  return (
    <>
      <motion.div
        className="fixed inset-0 z-50 bg-black/50"
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        exit={{ opacity: 0 }}
        transition={fade}
        onClick={cancelReview}
        aria-hidden="true"
      />
      <motion.div
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="confirm-title"
        className="fixed top-1/2 left-1/2 z-50 flex max-h-[85vh] w-[min(460px,calc(100vw-32px))] flex-col rounded-card border border-line bg-surface shadow-2xl"
        initial={{ opacity: 0, scale: 0.96, x: "-50%", y: "-46%" }}
        animate={{ opacity: 1, scale: 1, x: "-50%", y: "-50%" }}
        exit={{ opacity: 0, scale: 0.96, x: "-50%", y: "-46%" }}
        transition={spring}
      >
        {step === "review" ? (
          <div className="scroll-area space-y-4 overflow-y-auto p-5">
            <div>
              <h2 id="confirm-title" className="text-lg font-semibold text-text">
                Clean {total}?
              </h2>
              <p className="mt-1 text-muted">
                {plan.items.length} {plan.items.length === 1 ? "item" : "items"} selected. JClean
                checks each one again right before cleaning it.
              </p>
            </div>

            <ul className="divide-y divide-line rounded-control border border-line">
              {plan.byMethod.map((m) => (
                <li key={m.method} className="flex items-center justify-between gap-3 px-3 py-2.5">
                  <span className="text-text">
                    {METHOD_LINE[m.method]}
                    {m.method === "command" && plan.tools.length > 0 && (
                      <span className="text-muted">
                        {" "}
                        · {plan.tools.map((t) => TOOL_NAMES[t] ?? t).join(", ")}
                      </span>
                    )}
                  </span>
                  <span className="tabular shrink-0 text-muted">
                    {m.items} {m.items === 1 ? "item" : "items"} · {formatBytes(m.bytes)}
                  </span>
                </li>
              ))}
            </ul>

            {plan.byMethod.some((m) => m.method === "trash") && (
              <p className="text-xs text-muted">
                Items moved to the Trash can be put back. They free space once you empty the Trash.
              </p>
            )}

            {plan.runningApps.length > 0 && (
              <div className="rounded-control border border-review/40 bg-review/10 px-3 py-2.5">
                <p className="text-text">Close apps first: {plan.runningApps.join(", ")}</p>
                <p className="mt-0.5 text-xs text-muted">
                  Items they use will be skipped while they're open.{" "}
                  <button
                    type="button"
                    className="text-accent hover:underline"
                    onClick={() => {
                      void reviewClean(plan.items.map((i) => i.itemId));
                    }}
                  >
                    Check again
                  </button>
                </p>
              </div>
            )}

            {review.length > 0 && (
              <p className="flex gap-2 text-muted">
                <RiskIcon risk="review" />
                <span>
                  {review.length} {review.length === 1 ? "item needs" : "items need"} review. Make
                  sure you don't need {review.length === 1 ? "it" : "them"}.
                </span>
              </p>
            )}
            {admin.length > 0 && (
              <p className="text-xs text-muted">
                {admin.length} {admin.length === 1 ? "item is" : "items are"} owned by macOS. It will ask for your
                password once when cleaning starts.
              </p>
            )}
            {plan.skipped.length > 0 && (
              <details className="text-xs text-muted">
                <summary className="cursor-pointer">
                  {plan.skipped.length} selected{" "}
                  {plan.skipped.length === 1 ? "item can't" : "items can't"} be cleaned
                </summary>
                <ul className="mt-2 space-y-1">
                  {plan.skipped.map((s) => (
                    <li key={s.itemId}>
                      {label(s.itemId)}: {s.reason}
                    </li>
                  ))}
                </ul>
              </details>
            )}
          </div>
        ) : (
          <div className="scroll-area space-y-4 overflow-y-auto p-5">
            <div className="flex items-start gap-3">
              <RiskIcon risk="caution" size={20} />
              <div>
                <h2 id="confirm-title" className="text-lg font-semibold text-text">
                  These can't be recovered
                </h2>
                <p className="mt-1 text-muted">{RISK_COPY.caution.why}</p>
              </div>
            </div>
            <ul className="space-y-2.5">
              {caution.map((c) => {
                const item = byId.get(c.itemId);
                const rule = item ? rules.get(item.ruleId) : undefined;
                return (
                  <li
                    key={c.itemId}
                    className="rounded-control border border-caution/40 bg-caution/10 px-3 py-2.5"
                  >
                    <p className="flex justify-between gap-3 text-text">
                      <span className="truncate">{label(c.itemId)}</span>
                      <span className="tabular shrink-0">{formatBytes(c.bytes)}</span>
                    </p>
                    {rule && (
                      <p className="mt-1 text-xs text-muted">{rule.description.ifCleared}</p>
                    )}
                  </li>
                );
              })}
            </ul>
          </div>
        )}

        <div className="flex justify-end gap-2 border-t border-line p-4">
          <button
            type="button"
            onClick={() => {
              if (step === "caution") setStep("review");
              else cancelReview();
            }}
            className="h-9 rounded-control border border-line bg-raised px-4 text-text hover:border-muted"
          >
            {step === "caution" ? "Go back" : "Cancel"}
          </button>
          <button
            ref={primaryRef}
            type="button"
            onClick={go}
            disabled={plan.items.length === 0}
            className={`tabular h-9 rounded-control px-4 font-medium text-white hover:brightness-110 disabled:opacity-50 ${
              step === "caution" ? "bg-caution" : "bg-accent"
            }`}
          >
            {step === "caution" ? `Clean anyway · ${total}` : `Clean ${total}`}
          </button>
        </div>
      </motion.div>
    </>
  );
}
