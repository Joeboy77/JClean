import { CodeXml, Sparkles, type LucideIcon } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useRef, useState } from "react";
import type { Audience } from "../data/types";
import { fade, spring } from "../lib/motion";
import { checkFullDiskAccess, openLink, startScan, updateSettings } from "../state/engine";
import { useStore } from "../state/store";
import { LogoMark } from "./LogoMark";

type Step = "welcome" | "who" | "access";

const CHOICES: { id: Audience; title: string; body: string; icon: LucideIcon }[] = [
  {
    id: "developer",
    title: "I write code",
    body: "Includes caches from tools like npm, Xcode, Docker and Gradle.",
    icon: CodeXml,
  },
  {
    id: "everyday",
    title: "I just want space back",
    body: "Simple categories, safe choices.",
    icon: Sparkles,
  },
];

/** First launch only (spec §5.9): three short, skippable screens in the
 * compact-width layout, then the first quick scan starts. */
export function Onboarding() {
  const [step, setStep] = useState<Step>("welcome");
  const [mode, setModeChoice] = useState<Audience>("everyday");
  const fda = useStore((s) => s.fullDiskAccess);
  const done = useRef(false);

  const finish = () => {
    if (done.current) return;
    done.current = true;
    void updateSettings({ onboarded: true, mode }).then(() => {
      useStore.getState().setAudience(mode);
      startScan("quick");
    });
  };

  // The access screen detects when access is granted and moves on by itself.
  useEffect(() => {
    if (step !== "access") return;
    if (fda === true) {
      finish();
      return;
    }
    const timer = window.setInterval(() => {
      void checkFullDiskAccess();
    }, 2000);
    return () => {
      window.clearInterval(timer);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- `finish` is stable in effect
  }, [step, fda]);

  return (
    <motion.div
      className="fixed inset-0 z-[60] flex flex-col bg-bg"
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={fade}
      role="dialog"
      aria-modal="true"
      aria-label="Welcome to JClean"
    >
      <div data-tauri-drag-region className="h-11 shrink-0" />
      <div className="flex flex-1 items-center justify-center overflow-y-auto px-6 pb-10">
        <div className="w-full max-w-[380px]">
          <AnimatePresence mode="wait" initial={false}>
            <motion.div
              key={step}
              initial={{ opacity: 0, x: 24 }}
              animate={{ opacity: 1, x: 0 }}
              exit={{ opacity: 0, x: -24 }}
              transition={spring}
            >
              {step === "welcome" && (
                <div className="flex flex-col items-center text-center">
                  <span className="text-accent">
                    <LogoMark size={64} />
                  </span>
                  <h1 className="mt-6 text-2xl font-semibold text-text">
                    See what's filling your Mac, and clear it safely.
                  </h1>
                  <p className="mt-3 text-md text-muted">
                    No account, nothing leaves your Mac, and nothing is removed until you say so.
                  </p>
                  <button
                    type="button"
                    autoFocus
                    onClick={() => {
                      setStep("who");
                    }}
                    className="mt-8 h-10 w-full rounded-control bg-accent-strong font-medium text-white hover:brightness-110"
                  >
                    Get started
                  </button>
                </div>
              )}

              {step === "who" && (
                <div>
                  <h1 className="text-xl font-semibold text-text">Who's using JClean?</h1>
                  <div
                    role="radiogroup"
                    aria-label="Who's using JClean?"
                    className="mt-5 space-y-3"
                  >
                    {CHOICES.map((c) => {
                      const on = mode === c.id;
                      const Icon = c.icon;
                      return (
                        <button
                          key={c.id}
                          type="button"
                          role="radio"
                          aria-checked={on}
                          onClick={() => {
                            setModeChoice(c.id);
                          }}
                          className={`flex w-full items-start gap-3 rounded-card border p-4 text-left transition-colors ${
                            on
                              ? "border-accent bg-accent/10"
                              : "border-line bg-surface hover:border-muted"
                          }`}
                        >
                          <span className="grid size-9 shrink-0 place-items-center rounded-control bg-raised text-accent">
                            <Icon size={18} aria-hidden="true" />
                          </span>
                          <span>
                            <span className="block text-md font-medium text-text">{c.title}</span>
                            <span className="mt-0.5 block text-muted">{c.body}</span>
                          </span>
                        </button>
                      );
                    })}
                  </div>
                  <p className="mt-3 text-xs text-muted">
                    You can change this anytime in Settings.
                  </p>
                  <button
                    type="button"
                    onClick={() => {
                      setStep("access");
                    }}
                    className="mt-6 h-10 w-full rounded-control bg-accent-strong font-medium text-white hover:brightness-110"
                  >
                    Continue
                  </button>
                </div>
              )}

              {step === "access" && (
                <div>
                  <h1 className="text-xl font-semibold text-text">Let JClean see the whole disk</h1>
                  <p className="mt-3 text-muted">
                    macOS hides some folders, like Safari's cache, Mail downloads, iPhone backups
                    and the Trash, unless you allow Full Disk Access. Without it, those don't show
                    up.
                  </p>
                  <div className="mt-4 rounded-card border border-line bg-surface p-4">
                    <p className="text-text">
                      JClean only reads file sizes and dates, never file contents.
                    </p>
                    <p className="mt-1 text-muted">Nothing leaves your Mac.</p>
                  </div>
                  <ol className="mt-4 space-y-1 text-muted">
                    <li>1. Open System Settings.</li>
                    <li>2. Turn on JClean under Full Disk Access.</li>
                    <li>3. Come back here. JClean notices on its own.</li>
                  </ol>
                  <button
                    type="button"
                    onClick={() => {
                      openLink("fullDiskAccessSettings");
                    }}
                    className="mt-6 h-10 w-full rounded-control bg-accent-strong font-medium text-white hover:brightness-110"
                  >
                    Open System Settings
                  </button>
                  <p
                    className="mt-3 flex items-center justify-center gap-2 text-xs text-muted"
                    role="status"
                  >
                    <span className="size-1.5 animate-pulse rounded-full bg-accent" />
                    Waiting for access…
                  </p>
                </div>
              )}
            </motion.div>
          </AnimatePresence>

          <div className="mt-6 flex items-center justify-between">
            <ol className="flex gap-1.5" aria-label="Progress">
              {(["welcome", "who", "access"] as const).map((s) => (
                <li
                  key={s}
                  aria-current={s === step ? "step" : undefined}
                  className={`h-1.5 rounded-full transition-all ${s === step ? "w-5 bg-accent" : "w-1.5 bg-line"}`}
                />
              ))}
            </ol>
            <button
              type="button"
              onClick={() => {
                if (step === "welcome") setStep("who");
                else if (step === "who") setStep("access");
                else finish();
              }}
              className="text-muted hover:text-text"
            >
              {step === "access" ? "Skip for now" : "Skip"}
            </button>
          </div>
        </div>
      </div>
    </motion.div>
  );
}
