import { X } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useRef, type ReactNode } from "react";
import type { Settings } from "../../bindings";
import { fade, spring } from "../../lib/motion";
import { updateSettings } from "../../state/engine";
import { useStore, type SettingsSection } from "../../state/store";
import { Switch } from "../ui/Switch";
import { AboutSection, UpdatesSection } from "./AboutSection";
import { HistorySection } from "./HistorySection";
import { RulesSection } from "./RulesSection";
import { ScanningSection } from "./ScanningSection";
import { isWindows, words } from "../../lib/platform";

const SECTIONS: { id: SettingsSection; label: string }[] = [
  { id: "general", label: "General" },
  { id: "scanning", label: "Scanning" },
  { id: "cleaning", label: "Cleaning" },
  { id: "rules", label: "Rules" },
  { id: "history", label: "History" },
  { id: "updates", label: "Updates" },
  { id: "about", label: "About" },
];

export function Row({
  title,
  detail,
  children,
}: {
  title: string;
  detail?: string;
  children: ReactNode;
}) {
  return (
    <div className="flex items-center justify-between gap-6 py-3">
      <div className="min-w-0">
        <p className="text-text">{title}</p>
        {detail && <p className="mt-0.5 text-xs text-muted">{detail}</p>}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

export function Group({ title, children }: { title?: string; children: ReactNode }) {
  return (
    <section className="mb-6">
      {title && (
        <h3 className="mb-1 text-xs font-medium tracking-wide text-muted uppercase">{title}</h3>
      )}
      <div className="divide-y divide-line rounded-card border border-line bg-surface px-4">
        {children}
      </div>
    </section>
  );
}

export function Choice<T extends string>({
  value,
  options,
  onChange,
  label,
}: {
  value: T;
  options: { id: T; label: string }[];
  onChange: (v: T) => void;
  label: string;
}) {
  return (
    <div
      role="radiogroup"
      aria-label={label}
      className="flex rounded-control border border-line bg-raised p-0.5"
    >
      {options.map((o) => (
        <button
          key={o.id}
          type="button"
          role="radio"
          aria-checked={value === o.id}
          onClick={() => {
            onChange(o.id);
          }}
          className={`rounded-[6px] px-3 py-1 text-xs ${value === o.id ? "bg-surface text-text" : "text-muted hover:text-text"}`}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

function toggle(settings: Settings, key: keyof Settings) {
  return () => {
    void updateSettings({ [key]: !settings[key] });
  };
}

function GeneralSection({ s }: { s: Settings }) {
  return (
    <>
      <Group>
        <Row
          title="Mode"
          detail="Changes which storage is shown and how it's described. Never changes what's safe."
        >
          <Choice
            label="Mode"
            value={s.mode}
            options={[
              { id: "everyday", label: "Everyday" },
              { id: "developer", label: "Developer" },
            ]}
            onChange={(mode) => {
              useStore.getState().setAudience(mode);
              void updateSettings({ mode });
            }}
          />
        </Row>
        <Row
          title="Scan when JClean opens"
          detail="A quick scan of the usual places, in the background."
        >
          <Switch
            on={s.scanOnLaunch}
            label="Scan when JClean opens"
            onChange={toggle(s, "scanOnLaunch")}
          />
        </Row>
        <Row title="Open at login">
          <Switch
            on={s.launchAtLogin}
            label="Open at login"
            onChange={toggle(s, "launchAtLogin")}
          />
        </Row>
      </Group>
      <Group title={isWindows ? "System tray" : "Menu bar"}>
        <Row
          title={`Show JClean in the ${words.tray}`}
          detail="Free space, a quick scan, and closing the window keeps it there."
        >
          <Switch
            on={s.menuBarIcon}
            label={`Show JClean in the ${words.tray}`}
            onChange={toggle(s, "menuBarIcon")}
          />
        </Row>
        <Row title="Show free space next to the icon">
          <Switch
            on={s.menuBarFreeSpace}
            disabled={!s.menuBarIcon}
            label="Show free space next to the icon"
            onChange={toggle(s, "menuBarFreeSpace")}
          />
        </Row>
      </Group>
    </>
  );
}

function CleaningSection({ s }: { s: Settings }) {
  return (
    <Group>
      <Row
        title="Your own files"
        detail={`Old installers and large files. Moving them to the ${words.trash} means you can put them back.`}
      >
        <Choice
          label="Your own files"
          value={s.deleteUserFiles ? "delete" : "trash"}
          options={[
            { id: "trash", label: `Move to ${words.trash}` },
            { id: "delete", label: "Delete" },
          ]}
          onChange={(v) => {
            void updateSettings({ deleteUserFiles: v === "delete" });
          }}
        />
      </Row>
      <Row
        title="Ask before cleaning"
        detail="JClean always asks before cleaning anything marked caution, and when an app needs closing."
      >
        <Switch
          on={s.confirmBeforeCleaning}
          label="Ask before cleaning"
          onChange={toggle(s, "confirmBeforeCleaning")}
        />
      </Row>
    </Group>
  );
}

/** Settings (spec §5.11), over the whole window. */
export function SettingsView({ compact }: { compact: boolean }) {
  const section = useStore((s) => s.settingsOpen);
  const settings = useStore((s) => s.settings);
  const open = useStore((s) => s.openSettings);
  const closeRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!section) return;
    closeRef.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") open(null);
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
    };
  }, [section, open]);

  return (
    <AnimatePresence>
      {section && (
        <motion.div
          key="settings"
          role="dialog"
          aria-modal="true"
          aria-label="Settings"
          className="fixed inset-0 z-50 flex flex-col bg-bg"
          initial={{ opacity: 0, y: 12 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: 12 }}
          transition={spring}
        >
          <header
            data-tauri-drag-region
            className="flex h-14 shrink-0 items-center justify-center border-b border-line"
          >
            <h2 className="text-md font-semibold text-text">Settings</h2>
            <button
              ref={closeRef}
              type="button"
              aria-label="Close settings"
              onClick={() => {
                open(null);
              }}
              className="absolute right-4 grid size-8 place-items-center rounded-control text-muted hover:bg-raised hover:text-text"
            >
              <X size={16} aria-hidden="true" />
            </button>
          </header>
          <div className={`flex min-h-0 flex-1 ${compact ? "flex-col" : ""}`}>
            <nav
              aria-label="Settings sections"
              className={
                compact
                  ? "flex shrink-0 gap-1 overflow-x-auto border-b border-line px-3 py-2"
                  : "w-52 shrink-0 space-y-0.5 border-r border-line p-3"
              }
            >
              {SECTIONS.map((sec) => (
                <button
                  key={sec.id}
                  type="button"
                  aria-current={section === sec.id ? "page" : undefined}
                  onClick={() => {
                    open(sec.id);
                  }}
                  className={`shrink-0 rounded-row px-3 py-1.5 text-left ${compact ? "" : "w-full"} ${
                    section === sec.id ? "bg-raised text-text" : "text-muted hover:text-text"
                  }`}
                >
                  {sec.label}
                </button>
              ))}
            </nav>
            <AnimatePresence mode="wait" initial={false}>
              <motion.div
                key={section}
                className="scroll-area min-w-0 flex-1 overflow-y-auto px-6 py-6"
                initial={{ opacity: 0 }}
                animate={{ opacity: 1 }}
                exit={{ opacity: 0 }}
                transition={fade}
              >
                <div className="mx-auto max-w-2xl">
                  {!settings ? (
                    <p className="text-muted">Settings are available in the JClean app.</p>
                  ) : (
                    <>
                      {section === "general" && <GeneralSection s={settings} />}
                      {section === "scanning" && <ScanningSection s={settings} />}
                      {section === "cleaning" && <CleaningSection s={settings} />}
                      {section === "rules" && <RulesSection />}
                      {section === "history" && <HistorySection />}
                      {section === "updates" && <UpdatesSection s={settings} />}
                      {section === "about" && <AboutSection />}
                    </>
                  )}
                </div>
              </motion.div>
            </AnimatePresence>
          </div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
