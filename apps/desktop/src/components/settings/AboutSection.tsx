import { ExternalLink } from "lucide-react";
import { useEffect, useState } from "react";
import { useStore } from "../../state/store";
import { commands, type Settings } from "../../bindings";
import {
  checkForUpdate,
  installUpdate,
  isLive,
  openLink,
  updateSettings,
} from "../../state/engine";
import { LogoMark } from "../LogoMark";
import { Switch } from "../ui/Switch";
import { Group, Row } from "./SettingsView";

function useVersion() {
  const [version, setVersion] = useState("");
  useEffect(() => {
    if (isLive)
      void commands.appInfo().then((i) => {
        setVersion(i.version);
      });
  }, []);
  return version;
}

export function UpdatesSection({ s }: { s: Settings }) {
  const version = useVersion();
  const update = useStore((st) => st.update);
  const [status, setStatus] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  return (
    <>
      <Group>
        <Row
          title="Check for updates automatically"
          detail="Checks GitHub for new versions. It's JClean's only network request."
        >
          <Switch
            on={s.checkForUpdates}
            label="Check for updates automatically"
            onChange={() => void updateSettings({ checkForUpdates: !s.checkForUpdates })}
          />
        </Row>
        <Row title="Current version">
          <span className="tabular text-muted">{version}</span>
        </Row>
      </Group>
      <Group>
        <div className="py-3">
          {update ? (
            <>
              <p className="text-text">Version {update.version} is ready to install.</p>
              {update.notes && (
                <p className="mt-1 whitespace-pre-line text-xs text-muted">{update.notes}</p>
              )}
              <button
                type="button"
                disabled={busy}
                onClick={() => {
                  setBusy(true);
                  void installUpdate().then((e) => {
                    setBusy(false);
                    setStatus(e);
                  });
                }}
                className="mt-3 h-8 rounded-control bg-accent px-3 font-medium text-white hover:brightness-110 disabled:opacity-50"
              >
                {busy ? "Installing…" : "Restart to update"}
              </button>
            </>
          ) : (
            <div className="flex items-center justify-between gap-4">
              <p className="text-muted">{status ?? "Looks for a newer version now."}</p>
              <button
                type="button"
                disabled={busy || !s.checkForUpdates}
                onClick={() => {
                  setBusy(true);
                  void checkForUpdate().then((e) => {
                    setBusy(false);
                    setStatus(e ?? (useStore.getState().update ? null : "JClean is up to date."));
                  });
                }}
                className="h-8 shrink-0 rounded-control border border-line bg-raised px-3 text-text hover:border-muted disabled:opacity-50"
              >
                {busy ? "Checking…" : "Check now"}
              </button>
            </div>
          )}
        </div>
      </Group>
    </>
  );
}

/** About and the privacy statement (spec §10). */
export function AboutSection() {
  const version = useVersion();
  return (
    <div>
      <div className="mb-6 flex items-center gap-4">
        <span className="text-accent">
          <LogoMark size={44} />
        </span>
        <div>
          <p className="text-lg font-semibold text-text">JClean</p>
          <p className="tabular text-muted">Version {version}</p>
        </div>
      </div>
      <Group title="Privacy">
        <div className="space-y-2 py-3 text-text">
          <p>JClean reads file names, sizes and dates to calculate storage.</p>
          <p>It never reads file contents, never uploads anything, and has no account system.</p>
          <p>The only network request is the optional update check against GitHub.</p>
        </div>
      </Group>
      <Group title="Open source">
        <Row
          title="Free and open source"
          detail="Source code, releases and issue tracker on GitHub."
        >
          <button
            type="button"
            onClick={() => {
              openLink("repository");
            }}
            className="flex h-8 items-center gap-2 rounded-control border border-line bg-raised px-3 text-text hover:border-muted"
          >
            GitHub <ExternalLink size={13} aria-hidden="true" />
          </button>
        </Row>
      </Group>
    </div>
  );
}
