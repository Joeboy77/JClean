import { useSyncExternalStore } from "react";
import { type Platform, detectPlatform, prefersDeb, subscribe } from "../lib/platform";
import type { Download } from "../lib/release";

interface Props {
  mac: Download;
  windows: Download | null;
  linux: Download | null;
  releasesUrl: string;
}

interface Card {
  id: Exclude<Platform, "mobile">;
  name: string;
  requirement: string;
  download: Download | null;
}

/** Every platform side by side, with the visitor's own marked as
 * recommended (spec §12.2). The badge's line is always there and only
 * becomes visible, so detecting the platform after load moves nothing. */
export function PlatformCards({ mac, windows, linux, releasesUrl }: Props) {
  const platform = useSyncExternalStore(subscribe, detectPlatform, () => "mac" as const);
  const deb = useSyncExternalStore(subscribe, prefersDeb, () => false);

  // On Debian-family systems the .deb is the main Linux download.
  const linuxDownload: Download | null =
    linux && deb && linux.alternative
      ? {
          url: linux.alternative.url,
          detail: ".deb for Ubuntu and Debian",
          alternative: { url: linux.url, label: "AppImage for other distributions" },
        }
      : linux;

  const cards: Card[] = [
    {
      id: "mac",
      name: "Mac",
      requirement: "macOS 12 or later · Apple Silicon and Intel",
      download: mac,
    },
    { id: "windows", name: "Windows", requirement: "Windows 10 or 11 · 64-bit", download: windows },
    {
      id: "linux",
      name: "Linux",
      requirement: "64-bit · most distributions",
      download: linuxDownload,
    },
  ];

  return (
    <ul className="grid gap-4 sm:grid-cols-3">
      {cards.map((card) => {
        const recommended = card.id === platform;
        return (
          <li
            key={card.id}
            className={`flex flex-col rounded-card border bg-surface p-5 ${
              recommended ? "border-accent/70" : "border-line"
            }`}
          >
            <p
              className={`text-xs font-medium text-accent-text ${recommended ? "" : "invisible"}`}
              aria-hidden={!recommended}
            >
              Recommended for you
            </p>
            <h2 className="mt-1 text-lg font-semibold text-text">{card.name}</h2>
            <p className="mt-1 text-sm text-muted">{card.requirement}</p>
            <div className="mt-auto pt-5">
              {card.download ? (
                <>
                  <a
                    href={card.download.url}
                    className={`inline-flex h-10 w-full items-center justify-center rounded-control px-4 font-medium hover:brightness-110 ${
                      recommended
                        ? "bg-accent-strong text-white"
                        : "border border-line bg-raised text-text"
                    }`}
                  >
                    Download for {card.name}
                  </a>
                  <p className="mt-2 text-xs text-muted">{card.download.detail}</p>
                  {card.download.alternative && (
                    <a
                      href={card.download.alternative.url}
                      className="mt-1 inline-block text-xs text-accent hover:underline"
                    >
                      Or the {card.download.alternative.label}
                    </a>
                  )}
                </>
              ) : (
                <>
                  <a
                    href={releasesUrl}
                    className="inline-flex h-10 w-full items-center justify-center rounded-control border border-line px-4 text-muted hover:text-text"
                  >
                    Coming soon · Watch on GitHub
                  </a>
                  <p className="mt-2 text-xs text-muted">The Mac version is out now.</p>
                </>
              )}
            </div>
          </li>
        );
      })}
    </ul>
  );
}
