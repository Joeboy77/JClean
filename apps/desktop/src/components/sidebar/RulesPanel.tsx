import { FolderPlus } from "lucide-react";
import { useMemo } from "react";
import { RULES } from "../../data/catalog";
import type { Rule } from "../../data/types";
import { ruleLabel, visibleIn } from "../../state/selectors";
import { useStore } from "../../state/store";
import { RISK_COPY } from "../../data/riskCopy";
import { RiskIcon } from "../ui/RiskBadge";
import { RuleIcon } from "../ui/RuleIcon";

function Toggle({ on, label, onChange }: { on: boolean; label: string; onChange: () => void }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      aria-label={label}
      onClick={onChange}
      className={`relative h-5 w-9 shrink-0 rounded-full transition-colors ${on ? "bg-accent" : "bg-line"}`}
    >
      <span
        className={`absolute top-0.5 left-0.5 size-4 rounded-full bg-white shadow transition-transform duration-150 ${
          on ? "translate-x-4" : ""
        }`}
      />
    </button>
  );
}

/** Browse and switch detection rules (spec §5.2). Custom folders and rule
 * packs arrive with Settings in phase 5. */
export function RulesPanel() {
  const audience = useStore((s) => s.audience);
  const disabled = useStore((s) => s.disabledRules);
  const toggleRule = useStore((s) => s.toggleRule);

  const groups = useMemo(() => {
    const map = new Map<string, Rule[]>();
    for (const rule of RULES) {
      if (!visibleIn(rule, audience)) continue;
      const list = map.get(rule.group) ?? [];
      list.push(rule);
      map.set(rule.group, list);
    }
    return [...map.entries()].sort(([a], [b]) => a.localeCompare(b));
  }, [audience]);

  return (
    <div className="scroll-area -mx-2 min-h-0 flex-1 overflow-y-auto px-2 pb-3">
      <button
        type="button"
        disabled
        title="Adding your own folders arrives with Settings"
        className="mb-3 flex h-9 w-full items-center justify-center gap-2 rounded-control border border-dashed border-line text-muted disabled:cursor-not-allowed"
      >
        <FolderPlus size={15} aria-hidden="true" /> Add a folder
      </button>
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
                    <span className="block truncate text-text">{label}</span>
                    <span className="flex items-center gap-1 text-xs text-muted">
                      <RiskIcon risk={rule.risk} size={11} />
                      {RISK_COPY[rule.risk].title}
                    </span>
                  </span>
                  <Toggle
                    on={!disabled.has(rule.id)}
                    label={`Detect ${label}`}
                    onChange={() => {
                      toggleRule(rule.id);
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
