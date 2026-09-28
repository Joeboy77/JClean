import { useState, useSyncExternalStore } from "react";
import { detectPlatform as detect, prefersDeb, subscribe } from "../lib/platform";
import type { Download } from "../lib/release";

interface Props {
  mac: Download;
  /** The Windows installer, once a release has one. */
  windows: Download | null;
  /** The Linux AppImage (and .deb), once a release has them. */
  linux: Download | null;
  releasesUrl: string;
  size?: "large" | "normal";
}

const downloadIcon = (
  <svg width="18" height="18" viewBox="0 0 24 24" fill="none" aria-hidden="true">
    <path
      d="M12 3v12m0 0-5-5m5 5 5-5M5 20h14"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
    />
  </svg>
);

export function DownloadButton({ mac, windows, linux, releasesUrl, size = "large" }: Props) {
  // Rendered as "mac" on the server and switched after load. Every variant is
  // one button and one short line, so nothing below it moves.
  const platform = useSyncExternalStore(subscribe, detect, () => "mac" as const);
  const deb = useSyncExternalStore(subscribe, prefersDeb, () => false);
  // On Debian-family systems the .deb comes first, the AppImage second.
  const linuxMain =
    linux && deb && linux.alternative
      ? {
          url: linux.alternative.url,
          detail: ".deb for Ubuntu and Debian",
          other: { url: linux.url, label: "AppImage" },
        }
      : linux && { url: linux.url, detail: linux.detail, other: linux.alternative };
  const [copied, setCopied] = useState(false);

  const big = size === "large";
  const shape = `inline-flex items-center justify-center gap-2 rounded-control font-medium hover:brightness-110 ${
    big ? "h-12 px-6 text-md" : "h-10 px-5"
  }`;
  const button = `${shape} bg-accent-strong text-white`;
  const comingSoon = (platform === "windows" && !windows) || (platform === "linux" && !linux);

  return (
    <div className="flex flex-col items-center gap-2 sm:items-start">
      {platform === "mac" && (
        <a href={mac.url} className={button}>
          {downloadIcon}
          Download for Mac
        </a>
      )}
      {platform === "windows" && windows && (
        <a href={windows.url} className={button}>
          {downloadIcon}
          Download for Windows
        </a>
      )}
      {platform === "linux" && linuxMain && (
        <a href={linuxMain.url} className={button}>
          {downloadIcon}
          Download for Linux
        </a>
      )}
      {comingSoon && (
        <a href={releasesUrl} className={`${shape} border border-line bg-raised text-text`}>
          Coming to {platform === "windows" ? "Windows" : "Linux"} soon · Watch on GitHub
        </a>
      )}
      {platform === "mobile" && (
        <button
          type="button"
          className={button}
          onClick={() => {
            void navigator.clipboard.writeText(window.location.origin).then(() => {
              setCopied(true);
            });
          }}
        >
          {copied ? "Link copied" : "Copy link for your computer"}
        </button>
      )}
      <p className="text-sm text-muted" aria-live="polite">
        {platform === "mac" && mac.detail}
        {platform === "windows" && windows?.detail}
        {platform === "linux" && linuxMain && (
          <>
            {linuxMain.detail}
            {linuxMain.other && (
              <>
                {" · or the "}
                <a href={linuxMain.other.url} className="text-accent-text hover:underline">
                  {linuxMain.other.label}
                </a>
              </>
            )}
          </>
        )}
        {comingSoon && "The Mac version is out now."}
        {platform === "mobile" && "Open this page on your computer to download."}
      </p>
      <a href="/download" className="text-sm text-accent-text hover:underline">
        Other platforms and versions
      </a>
    </div>
  );
}
