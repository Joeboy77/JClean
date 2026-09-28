import { motion } from "motion/react";
import type { Audience } from "../../data/types";
import { spring } from "../../lib/motion";
import { setMode } from "../../state/engine";
import { useStore } from "../../state/store";

const MODES: { id: Audience; label: string }[] = [
  { id: "everyday", label: "Everyday" },
  { id: "developer", label: "Developer" },
];

/** Changes which rules and labels show. Never changes safety behavior (spec §2). */
export function ModeSwitch() {
  const audience = useStore((s) => s.audience);
  return (
    <div
      role="radiogroup"
      aria-label="Mode"
      className="flex rounded-control border border-line bg-raised p-0.5"
    >
      {MODES.map((m) => {
        const on = m.id === audience;
        return (
          <button
            key={m.id}
            type="button"
            role="radio"
            aria-checked={on}
            onClick={() => {
              setMode(m.id);
            }}
            className={`relative flex-1 rounded-[6px] px-3 py-1 text-xs transition-colors ${on ? "text-text" : "text-muted hover:text-text"}`}
          >
            {on && (
              <motion.span
                layoutId="mode-pill"
                transition={spring}
                className="absolute inset-0 rounded-[6px] bg-surface"
              />
            )}
            <span className="relative">{m.label}</span>
          </button>
        );
      })}
    </div>
  );
}
