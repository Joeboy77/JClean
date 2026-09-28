import { motion } from "motion/react";
import { spring } from "../../lib/motion";
import { useStore, type Tab } from "../../state/store";

const TABS: { id: Tab; label: string }[] = [
  { id: "categories", label: "Categories" },
  { id: "rules", label: "Rules" },
];

export function Tabs() {
  const tab = useStore((s) => s.tab);
  const setTab = useStore((s) => s.setTab);
  return (
    <div role="tablist" aria-label="Sidebar" className="flex gap-5 border-b border-line">
      {TABS.map((t) => {
        const active = t.id === tab;
        return (
          <button
            key={t.id}
            type="button"
            role="tab"
            id={`tab-${t.id}`}
            aria-selected={active}
            aria-controls={`panel-${t.id}`}
            onClick={() => {
              setTab(t.id);
            }}
            onKeyDown={(e) => {
              if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
                setTab(tab === "categories" ? "rules" : "categories");
                e.preventDefault();
              }
            }}
            className={`relative pb-2.5 text-md transition-colors ${active ? "text-text" : "text-muted hover:text-text"}`}
          >
            {t.label}
            {active && (
              <motion.span
                layoutId="tab-underline"
                transition={spring}
                className="absolute inset-x-0 -bottom-px h-0.5 rounded-full bg-accent"
              />
            )}
          </button>
        );
      })}
    </div>
  );
}
