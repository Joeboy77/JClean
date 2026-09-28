import { FolderOpen, X } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { RULES_BY_ID } from "../../data/catalog";
import { MOCK_HOME } from "../../data/mock";
import type { Method, Rule, StorageItem } from "../../data/types";
import { formatAgo, formatBytes, tildify } from "../../lib/format";
import { fade, spring } from "../../lib/motion";
import { itemName, ruleLabel } from "../../state/selectors";
import { useStore, type DrawerTarget } from "../../state/store";
import { RISK_COPY } from "../../data/riskCopy";
import { RiskBadge } from "../ui/RiskBadge";
import { RuleIcon } from "../ui/RuleIcon";

function methodCopy(method: Method, command: string | null): string {
  switch (method) {
    case "delete":
      return "Deleted permanently. The space is free right away.";
    case "trash":
      return "Moved to the Trash, so you can put it back until you empty the Trash.";
    case "command":
      return command ? `Cleared by the tool itself: ${command}` : "Cleared by the tool itself.";
    case "none":
      return "JClean doesn't clean this.";
  }
}

interface Resolved {
  rule: Rule;
  items: StorageItem[];
  title: string;
  subtitle: string;
}

function resolve(
  target: DrawerTarget,
  items: readonly StorageItem[],
  audience: "everyday" | "developer",
): Resolved | null {
  if (target.kind === "item") {
    const item = items.find((i) => i.id === target.id);
    const rule = item && RULES_BY_ID.get(item.ruleId);
    if (!item || !rule) return null;
    return { rule, items: [item], title: itemName(item), subtitle: ruleLabel(rule, audience) };
  }
  const rule = RULES_BY_ID.get(target.ruleId);
  if (!rule) return null;
  const list = items
    .filter((i) => i.ruleId === rule.id && i.risk === target.risk)
    .sort((a, b) => b.bytes - a.bytes);
  if (list.length === 0) return null;
  return { rule, items: list, title: ruleLabel(rule, audience), subtitle: rule.group };
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <section>
      <h3 className="mb-1 text-xs font-medium text-muted">{label}</h3>
      <div className="text-text">{children}</div>
    </section>
  );
}

function Locations({ items, developer }: { items: StorageItem[]; developer: boolean }) {
  const [shown, setShown] = useState(developer);
  const withPaths = items.filter((i) => i.path);
  if (withPaths.length === 0)
    return <p className="text-muted">Managed by its tool; there's no single folder to show.</p>;
  if (!shown) {
    return (
      <button
        type="button"
        onClick={() => {
          setShown(true);
        }}
        className="text-accent hover:underline"
      >
        Show location
      </button>
    );
  }
  const max = withPaths[0]?.bytes ?? 1;
  return (
    <ul className="space-y-2">
      {withPaths.slice(0, 8).map((item) => (
        <li key={item.id}>
          <div className="flex items-center gap-2">
            <span className="min-w-0 flex-1 truncate font-mono text-xs" title={item.path ?? ""}>
              {tildify(item.path ?? "", MOCK_HOME)}
            </span>
            <span className="tabular shrink-0 text-xs text-muted">{formatBytes(item.bytes)}</span>
          </div>
          {items.length > 1 && (
            <div className="mt-1 h-1 overflow-hidden rounded-full bg-raised">
              <div
                className="h-full rounded-full bg-accent/60"
                style={{ width: `${String((item.bytes / max) * 100)}%` }}
              />
            </div>
          )}
        </li>
      ))}
      {withPaths.length > 8 && (
        <li className="text-xs text-muted">and {withPaths.length - 8} more</li>
      )}
    </ul>
  );
}

interface DetailDrawerProps {
  compact: boolean;
}

/** Details for a row (spec §5.2). */
export function DetailDrawer({ compact }: DetailDrawerProps) {
  const target = useStore((s) => s.drawer);
  const items = useStore((s) => s.items);
  const audience = useStore((s) => s.audience);
  const close = useStore((s) => s.closeDrawer);
  const resolved = useMemo(
    () => (target ? resolve(target, items, audience) : null),
    [target, items, audience],
  );
  const closeRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (resolved) closeRef.current?.focus();
  }, [resolved]);

  useEffect(() => {
    if (!target) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") close();
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
    };
  }, [target, close]);

  const bytes = resolved?.items.reduce((n, i) => n + i.bytes, 0) ?? 0;
  const lastUsed = resolved?.items.reduce<number | null>(
    (m, i) => (i.lastUsed !== null && (m === null || i.lastUsed > m) ? i.lastUsed : m),
    null,
  );
  const blocked = resolved?.items.find((i) => i.blockedReason)?.blockedReason;
  const risk = resolved?.items[0]?.risk ?? resolved?.rule.risk ?? "safe";
  const shared = resolved?.items.some((i) => i.mayShareBlocks) ?? false;

  return (
    <AnimatePresence>
      {resolved && (
        <>
          <motion.div
            key="scrim"
            className="absolute inset-0 z-30 bg-bg/40"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={fade}
            onClick={close}
            aria-hidden="true"
          />
          <motion.aside
            key="drawer"
            role="dialog"
            aria-modal="true"
            aria-label={`Details: ${resolved.title}`}
            className={`absolute inset-y-0 right-0 z-40 flex flex-col border-l border-line bg-surface shadow-2xl ${
              compact ? "w-full" : "w-[420px] max-w-full rounded-l-card"
            }`}
            initial={{ x: "100%" }}
            animate={{ x: 0 }}
            exit={{ x: "100%" }}
            transition={spring}
          >
            <header data-tauri-drag-region className="flex items-start gap-3 px-5 pt-11 pb-4">
              <span
                className="grid size-10 shrink-0 place-items-center rounded-control bg-raised"
                style={{ color: `var(--cat-${resolved.rule.category})` }}
              >
                <RuleIcon name={resolved.rule.icon} size={20} />
              </span>
              <div className="min-w-0 flex-1">
                <h2 className="truncate text-lg font-semibold text-text">{resolved.title}</h2>
                <p className="truncate text-muted">{resolved.subtitle}</p>
              </div>
              <button
                ref={closeRef}
                type="button"
                onClick={close}
                aria-label="Close details"
                className="grid size-8 place-items-center rounded-control text-muted hover:bg-raised hover:text-text"
              >
                <X size={16} aria-hidden="true" />
              </button>
            </header>

            <div className="scroll-area flex-1 space-y-5 overflow-y-auto px-5 pb-6">
              <div className="flex flex-wrap items-center gap-3">
                <span className="tabular text-xl font-semibold text-text">
                  {shared ? "Up to " : ""}
                  {formatBytes(bytes)}
                </span>
                <RiskBadge risk={risk} />
              </div>
              {lastUsed != null && (
                <p className="-mt-3 text-muted">Last used {formatAgo(lastUsed)}</p>
              )}
              {blocked && (
                <p className="rounded-control border border-line bg-raised px-3 py-2 text-text">
                  {blocked}
                </p>
              )}

              <Field label="What it is">{resolved.rule.description.what}</Field>
              <Field label="If you clean it">{resolved.rule.description.ifCleared}</Field>
              <Field label="Comes back on its own">
                {resolved.rule.regenerates ? "Yes, when the app or tool needs it again." : "No."}
              </Field>
              <Field label={`Why it's “${RISK_COPY[risk].short.toLowerCase()}”`}>
                {RISK_COPY[risk].why}
              </Field>
              <Field label="How it's cleaned">
                {methodCopy(resolved.rule.method, resolved.rule.command)}
              </Field>
              <Field
                label={
                  resolved.items.length > 1
                    ? `Locations (${String(resolved.items.length)})`
                    : "Location"
                }
              >
                <Locations items={resolved.items} developer={audience === "developer"} />
              </Field>

              <div className="flex flex-wrap gap-2 pt-1">
                <button
                  type="button"
                  disabled
                  title="Available once JClean scans your Mac for real"
                  className="flex h-8 items-center gap-2 rounded-control border border-line bg-raised px-3 text-text disabled:opacity-50"
                >
                  <FolderOpen size={14} aria-hidden="true" /> Show in Finder
                </button>
              </div>
            </div>
          </motion.aside>
        </>
      )}
    </AnimatePresence>
  );
}
