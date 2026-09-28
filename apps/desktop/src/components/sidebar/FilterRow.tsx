import {
  CodeXml,
  Cpu,
  FileBox,
  PanelRightClose,
  PanelRightOpen,
  ShieldCheck,
  type LucideIcon,
} from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { useMemo } from "react";
import { filterCounts, type FilterKey } from "../../state/selectors";
import { useStore } from "../../state/store";
import { Tooltip } from "../ui/Tooltip";

const FILTERS: { key: FilterKey; label: string; icon: LucideIcon }[] = [
  { key: "developer", label: "Developer storage", icon: CodeXml },
  { key: "system", label: "System storage", icon: Cpu },
  { key: "large", label: "Large items (1 GB or more)", icon: FileBox },
  { key: "safeOnly", label: "Safe to clean only", icon: ShieldCheck },
];

interface FilterRowProps {
  compact: boolean;
  onToggleLayout: () => void;
}

export function FilterRow({ compact, onToggleLayout }: FilterRowProps) {
  const filters = useStore((s) => s.filters);
  const toggle = useStore((s) => s.toggleFilter);
  const items = useStore((s) => s.items);
  const counts = useMemo(() => filterCounts(items), [items]);

  return (
    <div className="flex items-center gap-1.5" role="toolbar" aria-label="Filters">
      {FILTERS.map(({ key, label, icon: Icon }, i) => {
        const on = filters[key];
        return (
          <Tooltip key={key} text={label} align={i === 0 ? "start" : "center"}>
            <button
              type="button"
              aria-pressed={on}
              aria-label={on ? `${label}, ${String(counts[key])} items` : label}
              onClick={() => {
                toggle(key);
              }}
              className={`relative grid size-8 place-items-center rounded-control border transition-colors ${
                on
                  ? "border-accent/60 bg-accent/15 text-text"
                  : "border-line bg-raised text-muted hover:text-text"
              }`}
            >
              <Icon size={16} strokeWidth={1.75} aria-hidden="true" />
              <AnimatePresence>
                {on && (
                  <motion.span
                    initial={{ scale: 0.6, opacity: 0 }}
                    animate={{ scale: 1, opacity: 1 }}
                    exit={{ scale: 0.6, opacity: 0 }}
                    transition={{ duration: 0.15 }}
                    aria-hidden="true"
                    className="tabular absolute -top-1.5 -right-1.5 grid h-4 min-w-4 place-items-center rounded-full bg-accent-strong px-1 text-[10px] font-semibold text-white"
                  >
                    {counts[key]}
                  </motion.span>
                )}
              </AnimatePresence>
            </button>
          </Tooltip>
        );
      })}
      <span className="flex-1" />
      <Tooltip text={compact ? "Show the disk map" : "Hide the disk map"} align="end">
        <button
          type="button"
          aria-label={compact ? "Show the disk map" : "Hide the disk map"}
          onClick={onToggleLayout}
          className="grid size-8 place-items-center rounded-control border border-line bg-raised text-muted transition-colors hover:text-text"
        >
          {compact ? (
            <PanelRightOpen size={16} strokeWidth={1.75} aria-hidden="true" />
          ) : (
            <PanelRightClose size={16} strokeWidth={1.75} aria-hidden="true" />
          )}
        </button>
      </Tooltip>
    </div>
  );
}
