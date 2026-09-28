import { ShieldAlert } from "lucide-react";
import { useEffect, useRef } from "react";
import { checkFullDiskAccess, openLink, startScan } from "../../state/engine";
import { useStore } from "../../state/store";

/** Shown while Full Disk Access is missing (spec §5.5 "permission missing",
 * §11). Checks every 2 s while visible and rescans once it's granted. */
export function AccessBanner() {
  const fda = useStore((s) => s.fullDiskAccess);
  const onboarded = useStore((s) => s.settings?.onboarded === true);
  const missing = fda === false && onboarded;
  const wasMissing = useRef(false);

  useEffect(() => {
    if (!missing) {
      if (wasMissing.current) startScan("quick");
      wasMissing.current = false;
      return;
    }
    wasMissing.current = true;
    const timer = window.setInterval(() => {
      void checkFullDiskAccess();
    }, 2000);
    return () => {
      window.clearInterval(timer);
    };
  }, [missing]);

  if (!missing) return null;
  return (
    <div
      className="flex items-start gap-3 rounded-card border border-review/40 bg-review/10 p-3"
      role="status"
    >
      <ShieldAlert size={16} className="mt-0.5 shrink-0 text-review" aria-hidden="true" />
      <div className="min-w-0 flex-1">
        <p className="text-text">Some storage is hidden from JClean.</p>
        <p className="mt-0.5 text-xs text-muted">
          Allow Full Disk Access to include Safari, Mail, iPhone backups and the Trash.
        </p>
        <button
          type="button"
          onClick={() => {
            openLink("fullDiskAccessSettings");
          }}
          className="mt-2 text-xs font-medium text-accent hover:underline"
        >
          Open System Settings
        </button>
      </div>
    </div>
  );
}
