import { useState, useSyncExternalStore } from "react";

type Platform = "mac" | "windows" | "linux" | "mobile";

interface Props {
  url: string;
  /** e.g. "macOS 12 or later · 8.9 MB" */
  detail: string;
  releasesUrl: string;
  size?: "large" | "normal";
}

/** Picks the right action for the visitor's device (spec §12.2). */
function detect(): Platform {
  const nav = navigator as Navigator & { userAgentData?: { platform?: string; mobile?: boolean } };
  const hint = nav.userAgentData?.platform?.toLowerCase() ?? "";
  const ua = navigator.userAgent.toLowerCase();
  const touchMac = ua.includes("macintosh") && navigator.maxTouchPoints > 1; // iPadOS reports as Mac
  if (nav.userAgentData?.mobile || /iphone|ipad|ipod|android/.test(ua) || touchMac) return "mobile";
  if (hint.includes("mac") || ua.includes("mac os")) return "mac";
  if (hint.includes("win") || ua.includes("windows")) return "windows";
  if (hint.includes("linux") || ua.includes("linux") || ua.includes("x11")) return "linux";
  return "mac";
}

// The device never changes while the page is open.
const subscribe = () => () => undefined;

export function DownloadButton({ url, detail, releasesUrl, size = "large" }: Props) {
  // Rendered as "mac" on the server and switched after load. Every variant is
  // one button and one short line, so nothing below it moves.
  const platform = useSyncExternalStore(subscribe, detect, () => "mac" as const);
  const [copied, setCopied] = useState(false);

  const big = size === "large";
  const shape = `inline-flex items-center justify-center gap-2 rounded-control font-medium hover:brightness-110 ${
    big ? "h-12 px-6 text-md" : "h-10 px-5"
  }`;
  const button = `${shape} bg-accent-strong text-white`;

  return (
    <div className="flex flex-col items-center gap-2 sm:items-start">
      {platform === "mac" && (
        <a href={url} className={button}>
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" aria-hidden="true">
            <path
              d="M12 3v12m0 0-5-5m5 5 5-5M5 20h14"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
          Download for Mac
        </a>
      )}
      {(platform === "windows" || platform === "linux") && (
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
        {platform === "mac" && detail}
        {(platform === "windows" || platform === "linux") && "The Mac version is out now."}
        {platform === "mobile" && "Open this page on your computer to download."}
      </p>
      <a href="/download" className="text-sm text-accent hover:underline">
        Other platforms and versions
      </a>
    </div>
  );
}
