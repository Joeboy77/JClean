import { AnimatePresence, MotionConfig, motion } from "motion/react";
import { useEffect, useRef } from "react";
import { Canvas } from "./components/canvas/Canvas";
import { ConfirmSheet } from "./components/ConfirmSheet";
import { ConnectorLine } from "./components/ConnectorLine";
import { DetailDrawer } from "./components/drawer/DetailDrawer";
import { Sidebar } from "./components/sidebar/Sidebar";
import { useWindowMode } from "./lib/windowMode";
import { init } from "./state/engine";
import { useStore } from "./state/store";

export function App() {
  const { compact, showCanvas, toggle, onCanvasHidden } = useWindowMode();
  const searchRef = useRef<HTMLInputElement>(null);
  const setTab = useStore((s) => s.setTab);

  // Show the last scan right away, then refresh it (spec §4.1, §10).
  useEffect(() => {
    const autoScan = !(
      import.meta.env.DEV &&
      (import.meta.env.VITE_NO_AUTOSCAN || import.meta.env.VITE_DEMO)
    );
    void init({ autoScan });
  }, []);

  // ⌘F focuses search.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "f") {
        e.preventDefault();
        setTab("categories");
        requestAnimationFrame(() => searchRef.current?.focus());
      }
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
    };
  }, [setTab]);

  return (
    // Every animation respects the system's reduced-motion setting.
    <MotionConfig reducedMotion="user">
      <div className="relative flex h-full overflow-hidden">
        <div className={`relative h-full ${compact ? "w-full" : ""}`}>
          <Sidebar ref={searchRef} compact={compact} onToggleLayout={toggle} />
          {compact && <DetailDrawer compact />}
        </div>
        <AnimatePresence onExitComplete={onCanvasHidden}>
          {showCanvas && (
            <motion.div
              key="canvas"
              className="relative flex h-full min-w-0 flex-1"
              initial={{ opacity: 0, x: 32 }}
              animate={{ opacity: 1, x: 0 }}
              exit={{ opacity: 0, x: 32 }}
              transition={{ duration: 0.18, ease: [0.16, 1, 0.3, 1] }}
            >
              <Canvas />
              <DetailDrawer compact={false} />
            </motion.div>
          )}
        </AnimatePresence>
        {showCanvas && <ConnectorLine />}
        <ConfirmSheet />
      </div>
    </MotionConfig>
  );
}
