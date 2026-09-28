import { CircleCheck, Info, OctagonAlert, TriangleAlert } from "lucide-react";
import { RISK_COPY } from "../../data/riskCopy";
import type { Risk } from "../../data/types";

const ICON = { safe: CircleCheck, review: TriangleAlert, caution: OctagonAlert, info: Info };
const COLOR = {
  safe: "text-safe",
  review: "text-review",
  caution: "text-caution",
  info: "text-muted",
};

/** Risk is never shown by colour alone: always an icon and a label too (spec §5.12). */
export function RiskIcon({ risk, size = 14 }: { risk: Risk; size?: number }) {
  const Icon = ICON[risk];
  return <Icon size={size} strokeWidth={2} aria-hidden="true" className={COLOR[risk]} />;
}

export function RiskBadge({ risk }: { risk: Risk }) {
  return (
    <span className="inline-flex items-center gap-1.5 rounded-row bg-raised px-2 py-0.5 text-xs text-text">
      <RiskIcon risk={risk} size={12} />
      {RISK_COPY[risk].title}
    </span>
  );
}
