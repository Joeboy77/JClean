import type { ReactNode } from "react";

interface TooltipProps {
  text: string;
  children: ReactNode;
  side?: "top" | "bottom";
  align?: "start" | "center" | "end";
  /** Stretch to the parent's width, for full-width controls. */
  block?: boolean;
}

/** Appears on hover and keyboard focus. The text is also the control's
 * accessible description, so it isn't hidden from VoiceOver. */
export function Tooltip({
  text,
  children,
  side = "bottom",
  align = "center",
  block = false,
}: TooltipProps) {
  const position = side === "bottom" ? "top-full mt-2" : "bottom-full mb-2";
  const alignment =
    align === "start" ? "left-0" : align === "end" ? "right-0" : "left-1/2 -translate-x-1/2";
  return (
    <span className={`group/tip relative ${block ? "flex w-full" : "inline-flex"}`}>
      {children}
      <span
        role="tooltip"
        className={`pointer-events-none absolute z-50 w-max max-w-64 rounded-control border border-line bg-raised px-2.5 py-1.5 text-xs text-text opacity-0 shadow-lg transition-opacity duration-150 group-hover/tip:opacity-100 group-focus-within/tip:opacity-100 ${position} ${alignment}`}
      >
        {text}
      </span>
    </span>
  );
}
