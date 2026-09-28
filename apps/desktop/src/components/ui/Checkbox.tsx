import { motion } from "motion/react";
import type { Check } from "../../state/selectors";

interface CheckboxProps {
  state: Check;
  disabled?: boolean;
  label: string;
  onToggle: () => void;
}

/** Tri-state checkbox whose tick draws in (spec §5.8). Not a tab stop: the
 * list moves focus between rows and Space toggles. */
export function Checkbox({ state, disabled = false, label, onToggle }: CheckboxProps) {
  const on = state !== "none";
  return (
    <button
      type="button"
      role="checkbox"
      tabIndex={-1}
      aria-checked={state === "some" ? "mixed" : state === "all"}
      aria-label={label}
      disabled={disabled}
      onClick={(e) => {
        e.stopPropagation();
        onToggle();
      }}
      className={`grid size-4 shrink-0 place-items-center rounded-[4px] border transition-colors duration-150 ${
        on ? "border-accent bg-accent" : "border-line bg-transparent hover:border-muted"
      } disabled:cursor-not-allowed disabled:opacity-40`}
    >
      <svg viewBox="0 0 12 12" className="size-3" aria-hidden="true">
        {state === "some" ? (
          <motion.path
            d="M3 6h6"
            stroke="white"
            strokeWidth={1.8}
            strokeLinecap="round"
            initial={{ pathLength: 0 }}
            animate={{ pathLength: 1 }}
            transition={{ duration: 0.15 }}
          />
        ) : (
          <motion.path
            d="M2.5 6.2 5 8.6 9.6 3.6"
            fill="none"
            stroke="white"
            strokeWidth={1.8}
            strokeLinecap="round"
            strokeLinejoin="round"
            initial={false}
            animate={{ pathLength: on ? 1 : 0, opacity: on ? 1 : 0 }}
            transition={{ duration: 0.18 }}
          />
        )}
      </svg>
    </button>
  );
}
