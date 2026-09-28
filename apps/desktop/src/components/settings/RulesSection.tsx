import { FileJson, FolderPlus, X } from "lucide-react";
import { useMemo, useState } from "react";
import { tildify } from "../../lib/format";
import { addCustomFolder, importRulePack, pickFolder, removeCustomRule } from "../../state/engine";
import { useStore } from "../../state/store";
import { RulesPanel } from "../sidebar/RulesPanel";
import { Choice, Group } from "./SettingsView";

type Risk = "safe" | "review" | "caution";

/** Custom folders and rule packs (spec §6.4), then every rule's switch. */
export function RulesSection() {
  const rules = useStore((s) => s.rules);
  const home = useStore((s) => s.home);
  const custom = useMemo(() => [...rules.values()].filter((r) => r.custom), [rules]);
  const [draft, setDraft] = useState<{ path: string; name: string; risk: Risk } | null>(null);
  const [error, setError] = useState<string | null>(null);

  return (
    <>
      <Group title="Your folders">
        <div className="py-3">
          <p className="text-xs text-muted">
            Folders you add are always moved to the Trash when cleaned, so you can put them back.
          </p>
          <ul className="mt-3 space-y-2">
            {custom.length === 0 && <li className="text-muted">None yet.</li>}
            {custom.map((r) => (
              <li key={r.id} className="flex items-center gap-2">
                <span className="rounded-row bg-accent/15 px-1.5 py-0.5 text-[10px] font-medium text-accent">
                  Custom
                </span>
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-text">{r.labels.everyday}</span>
                  <span className="block truncate text-xs text-muted">{r.description.what}</span>
                </span>
                <button
                  type="button"
                  aria-label={`Remove ${r.labels.everyday}`}
                  onClick={() => {
                    void removeCustomRule(r.id).then(setError);
                  }}
                  className="grid size-6 place-items-center rounded-row text-muted hover:bg-raised hover:text-text"
                >
                  <X size={13} aria-hidden="true" />
                </button>
              </li>
            ))}
          </ul>

          {draft ? (
            <div className="mt-4 space-y-3 rounded-control border border-line bg-bg p-3">
              <p className="truncate font-mono text-xs text-muted">{tildify(draft.path, home)}</p>
              <label className="block">
                <span className="text-xs text-muted">Name</span>
                <input
                  autoFocus
                  value={draft.name}
                  onChange={(e) => {
                    setDraft({ ...draft, name: e.target.value });
                  }}
                  className="mt-1 h-8 w-full rounded-control border border-line bg-raised px-2 text-text outline-none focus:border-accent"
                />
              </label>
              <div className="flex items-center justify-between gap-3">
                <span className="text-xs text-muted">How careful to be</span>
                <Choice
                  label="Risk"
                  value={draft.risk}
                  options={[
                    { id: "safe", label: "Safe" },
                    { id: "review", label: "Review" },
                    { id: "caution", label: "Caution" },
                  ]}
                  onChange={(risk) => {
                    setDraft({ ...draft, risk });
                  }}
                />
              </div>
              <div className="flex justify-end gap-2">
                <button
                  type="button"
                  onClick={() => {
                    setDraft(null);
                  }}
                  className="h-8 rounded-control border border-line bg-raised px-3 text-text"
                >
                  Cancel
                </button>
                <button
                  type="button"
                  disabled={!draft.name.trim()}
                  onClick={() => {
                    void addCustomFolder(draft.path, draft.name.trim(), draft.risk).then((e) => {
                      setError(e);
                      if (!e) setDraft(null);
                    });
                  }}
                  className="h-8 rounded-control bg-accent px-3 font-medium text-white disabled:opacity-50"
                >
                  Add folder
                </button>
              </div>
            </div>
          ) : (
            <div className="mt-4 flex flex-wrap gap-2">
              <button
                type="button"
                onClick={() => {
                  void pickFolder().then((path) => {
                    if (path) {
                      setError(null);
                      setDraft({ path, name: path.split("/").at(-1) ?? "Folder", risk: "review" });
                    }
                  });
                }}
                className="flex h-8 items-center gap-2 rounded-control border border-line bg-raised px-3 text-text hover:border-muted"
              >
                <FolderPlus size={14} aria-hidden="true" /> Add a folder
              </button>
              <button
                type="button"
                onClick={() => {
                  void importRulePack().then(setError);
                }}
                className="flex h-8 items-center gap-2 rounded-control border border-line bg-raised px-3 text-text hover:border-muted"
              >
                <FileJson size={14} aria-hidden="true" /> Import a rule pack
              </button>
            </div>
          )}
          {error && (
            <p className="mt-3 text-xs text-caution" role="alert">
              {error}
            </p>
          )}
        </div>
      </Group>
      <h3 className="mb-2 text-xs font-medium tracking-wide text-muted uppercase">
        What JClean looks for
      </h3>
      <RulesPanel embedded />
    </>
  );
}
