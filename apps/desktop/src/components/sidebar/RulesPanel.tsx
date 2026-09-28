import { FolderPlus } from "lucide-react";
import { useMemo } from "react";
import type { Rule } from "../../data/types";
import { ruleLabel, visibleIn } from "../../state/selectors";
import { toggleRuleEnabled } from "../../state/engine";
import { useStore } from "../../state/store";
import { RISK_COPY } from "../../data/riskCopy";
import { RiskIcon } from "../ui/RiskBadge";
import { RuleIcon } from "../ui/RuleIcon";
import { Switch } from "../ui/Switch";

/** Browse and switch detection rules (spec §5.2). Custom folders and rule
 * packs arrive with Settings in phase 5. */
export function RulesPanel({ embedded = false }: { embedded?: boolean }) {
  const audience = useStore((s) => s.audience);
  const disabled = useStore((s) => s.disabledRules);
  const rules = useStore((s) => s.rules);
  const openSettings = useStore((s) => s.openSettings);

  const groups = useMemo(() => {
    const map = new Map<string, Rule[]>();
    for (const rule of rules.values()) {
      if (!visibleIn(rule, audience)) continue;
      const list = map.get(rule.group) ?? [];
      list.push(rule);
      map.set(rule.group, list);
    }
    return [...map.entries()].sort(([a], [b]) => a.localeCompare(b));
  }, [audience, rules]);

  return (
    <div className={embedded ? "" : "scroll-area -mx-2 min-h-0 flex-1 overflow-y-auto px-2 pb-3"}>
      {!embedded && (
        <button
          type="button"
          onClick={() => {
            openSettings("rules");
          }}
          className="mb-3 flex h-9 w-full items-center justify-center gap-2 rounded-control border border-dashed border-line text-muted hover:border-muted hover:text-text"
        >
          <FolderPlus size={15} aria-hidden="true" /> Add a folder
        </button>
      )}
      {groups.map(([group, rules]) => (
        <section key={group} className="mb-3">
          <h3 className="px-2 pb-1 text-xs font-medium tracking-wide text-muted uppercase">
            {group}
          </h3>
          <ul>
            {rules.map((rule) => {
              const label = ruleLabel(rule, audience);
              return (
                <li
                  key={rule.id}
                  className="flex items-center gap-3 rounded-row px-2 py-2 hover:bg-raised/60"
                >
                  <span
                    className="grid size-7 shrink-0 place-items-center rounded-row bg-raised"
                    style={{ color: `var(--cat-${rule.category})` }}
                  >
                    <RuleIcon name={rule.icon} size={15} />
                  </span>
                  <span className="min-w-0 flex-1">
                    <span className="flex items-center gap-1.5">
                      <span className="truncate text-text">{label}</span>
                      {rule.custom && (
                        <span className="shrink-0 rounded-row bg-accent/15 px-1.5 text-[10px] font-medium text-accent">
                          Custom
                        </span>
                      )}
                    </span>
                    <span className="flex items-center gap-1 text-xs text-muted">
                      <RiskIcon risk={rule.risk} size={11} />
                      {RISK_COPY[rule.risk].title}
                    </span>
                  </span>
                  <Switch
                    on={!disabled.has(rule.id)}
                    label={`Detect ${label}`}
                    onChange={() => {
                      toggleRuleEnabled(rule.id);
                    }}
                  />
                </li>
              );
            })}
          </ul>
        </section>
      ))}
    </div>
  );
}
