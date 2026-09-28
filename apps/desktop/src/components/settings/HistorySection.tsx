import { ChevronRight, Download } from "lucide-react";
import { useEffect, useState } from "react";
import { commands, type ActionDto, type CleanupDto } from "../../bindings";
import { formatBytes } from "../../lib/format";
import { isLive } from "../../state/engine";
import { useStore } from "../../state/store";
import { words } from "../../lib/platform";

function when(secs: number): string {
  return new Date(secs * 1000).toLocaleString(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  });
}

/** Past cleanups and their deletion logs (spec §5.4, §7.5). */
export function HistorySection() {
  const rules = useStore((s) => s.rules);
  const audience = useStore((s) => s.audience);
  const [cleanups, setCleanups] = useState<CleanupDto[] | null>(null);
  const [open, setOpen] = useState<number | null>(null);
  const [actions, setActions] = useState<ActionDto[]>([]);
  const [note, setNote] = useState<string | null>(null);

  useEffect(() => {
    if (!isLive) return;
    void commands.historyCleanups().then((r) => {
      setCleanups(r.status === "ok" ? r.data.filter((c) => !c.dryRun) : []);
    });
  }, []);

  const toggle = (id: number) => {
    if (open === id) {
      setOpen(null);
      return;
    }
    setOpen(id);
    setActions([]);
    void commands.historyActions(id).then((r) => {
      if (r.status === "ok") setActions(r.data);
    });
  };

  const label = (ruleId: string) => {
    const rule = rules.get(ruleId);
    return rule
      ? audience === "developer"
        ? rule.labels.developer
        : rule.labels.everyday
      : ruleId;
  };

  return (
    <div>
      <div className="mb-4 flex items-center justify-between">
        <p className="text-muted">
          Every clean is logged on this {words.computer}. Nothing is sent anywhere.
        </p>
        <button
          type="button"
          onClick={() => {
            void commands.exportHistory().then((r) => {
              setNote(r.status === "ok" ? (r.data ? "Saved." : null) : r.error);
            });
          }}
          className="flex h-8 shrink-0 items-center gap-2 rounded-control border border-line bg-raised px-3 text-text hover:border-muted"
        >
          <Download size={14} aria-hidden="true" /> Export as CSV
        </button>
      </div>
      {note && <p className="mb-3 text-xs text-muted">{note}</p>}
      {cleanups === null && <p className="text-muted">Loading…</p>}
      {cleanups?.length === 0 && <p className="text-muted">No cleanups yet.</p>}
      <ul className="divide-y divide-line rounded-card border border-line bg-surface">
        {cleanups?.map((c) => (
          <li key={c.id}>
            <button
              type="button"
              aria-expanded={open === c.id}
              onClick={() => {
                toggle(c.id);
              }}
              className="flex w-full items-center gap-3 px-4 py-3 text-left hover:bg-raised/50"
            >
              <ChevronRight
                size={14}
                aria-hidden="true"
                className={`text-muted transition-transform ${open === c.id ? "rotate-90" : ""}`}
              />
              <span className="flex-1 text-text">{when(c.time)}</span>
              <span className="tabular text-muted">{formatBytes(c.freedBytes ?? 0)} cleaned</span>
            </button>
            {open === c.id && (
              <ul className="space-y-1.5 px-4 pb-3 pl-11">
                {actions.map((a, i) => (
                  <li key={i} className="flex gap-3 text-xs">
                    <span className="min-w-0 flex-1">
                      <span className="block truncate text-text">{label(a.ruleId)}</span>
                      <span className="block truncate font-mono text-muted" title={a.path}>
                        {a.path}
                      </span>
                      {a.error && <span className="block text-caution">{a.error}</span>}
                    </span>
                    <span className="tabular shrink-0 text-muted">
                      {a.outcome === "cleaned" ? formatBytes(a.bytes) : a.outcome}
                    </span>
                  </li>
                ))}
              </ul>
            )}
          </li>
        ))}
      </ul>
    </div>
  );
}
