import { FolderPlus, X } from "lucide-react";
import type { Settings } from "../../bindings";
import { tildify } from "../../lib/format";
import { pickFolder, updateSettings } from "../../state/engine";
import { useStore } from "../../state/store";
import { Switch } from "../ui/Switch";
import { Group, Row } from "./SettingsView";

const THRESHOLDS = [30, 60, 90, 180, 365];

function FolderList({
  folders,
  empty,
  onChange,
  addLabel,
}: {
  folders: string[];
  empty: string;
  onChange: (folders: string[]) => void;
  addLabel: string;
}) {
  const home = useStore((s) => s.home);
  return (
    <div className="py-3">
      <ul className="space-y-1.5">
        {folders.length === 0 && <li className="text-muted">{empty}</li>}
        {folders.map((f) => (
          <li key={f} className="flex items-center gap-2">
            <span className="min-w-0 flex-1 truncate font-mono text-xs text-text" title={f}>
              {tildify(f, home)}
            </span>
            <button
              type="button"
              aria-label={`Remove ${f}`}
              onClick={() => {
                onChange(folders.filter((x) => x !== f));
              }}
              className="grid size-6 place-items-center rounded-row text-muted hover:bg-raised hover:text-text"
            >
              <X size={13} aria-hidden="true" />
            </button>
          </li>
        ))}
      </ul>
      <button
        type="button"
        onClick={() => {
          void pickFolder().then((path) => {
            if (path && !folders.includes(path)) onChange([...folders, path]);
          });
        }}
        className="mt-3 flex h-8 items-center gap-2 rounded-control border border-line bg-raised px-3 text-text hover:border-muted"
      >
        <FolderPlus size={14} aria-hidden="true" /> {addLabel}
      </button>
    </div>
  );
}

export function ScanningSection({ s }: { s: Settings }) {
  return (
    <>
      <Group title="Where to look for projects and large files">
        <FolderList
          folders={s.projectRoots}
          empty="Your home folder"
          addLabel="Add a folder"
          onChange={(projectRoots) => void updateSettings({ projectRoots })}
        />
      </Group>
      <Group title="Folders to skip">
        <FolderList
          folders={s.excludedFolders}
          empty="None. Library, the Trash and iCloud folders are always skipped when looking for projects."
          addLabel="Skip a folder"
          onChange={(excludedFolders) => void updateSettings({ excludedFolders })}
        />
      </Group>
      <Group>
        <Row
          title="A project is inactive after"
          detail="Build folders of inactive projects are safe to clean and ticked for you."
        >
          <select
            value={s.inactiveAfterDays}
            aria-label="A project is inactive after"
            onChange={(e) => void updateSettings({ inactiveAfterDays: Number(e.target.value) })}
            className="h-8 rounded-control border border-line bg-raised px-2 text-text"
          >
            {THRESHOLDS.map((d) => (
              <option key={d} value={d}>
                {d === 365 ? "1 year" : `${String(d)} days`}
              </option>
            ))}
          </select>
        </Row>
        <Row title="Include external drives" detail="Coming in a later version.">
          <Switch
            on={s.includeExternalDrives}
            disabled
            label="Include external drives"
            onChange={() => undefined}
          />
        </Row>
      </Group>
    </>
  );
}
