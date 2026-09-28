import { isTauri } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";
import { commands } from "../bindings";

/** Below this width only the sidebar fits (spec §5.1). */
export const COMPACT_BREAKPOINT = 760;

/**
 * Expanded/compact layout. `compact` follows the window width; `toggle`
 * (the collapse/expand button) first slides the canvas away, then asks the
 * shell to animate the window, so the sidebar never moves.
 */
export function useWindowMode() {
  const [compact, setCompact] = useState(() => window.innerWidth < COMPACT_BREAKPOINT);
  const [collapsing, setCollapsing] = useState(false);
  // In a plain browser (no Tauri) the button just switches the layout.
  const [forced, setForced] = useState(false);

  useEffect(() => {
    const onResize = () => {
      const narrow = window.innerWidth < COMPACT_BREAKPOINT;
      setCompact(narrow);
      if (narrow) setCollapsing(false);
    };
    window.addEventListener("resize", onResize);
    return () => {
      window.removeEventListener("resize", onResize);
    };
  }, []);

  const isCompact = compact || forced;

  const toggle = useCallback(() => {
    if (!isTauri()) {
      setForced((f) => !f);
      return;
    }
    if (isCompact) {
      void commands.setWindowMode("expanded");
    } else {
      setCollapsing(true);
    }
  }, [isCompact]);

  /** Called when the canvas has finished sliding out. */
  const onCanvasHidden = useCallback(() => {
    if (collapsing) void commands.setWindowMode("compact");
  }, [collapsing]);

  return { compact: isCompact, showCanvas: !isCompact && !collapsing, toggle, onCanvasHidden };
}
