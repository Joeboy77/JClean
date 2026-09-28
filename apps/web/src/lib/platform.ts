// Which computer the visitor is on (spec §12.2), shared by the download
// button and the platform cards. The server renders the Mac choice; the
// browser switches after load, in place.

export type Platform = "mac" | "windows" | "linux" | "mobile";

interface NavigatorWithHints extends Navigator {
  userAgentData?: { platform?: string; mobile?: boolean };
}

export function detectPlatform(): Platform {
  const nav = navigator as NavigatorWithHints;
  const hint = nav.userAgentData?.platform?.toLowerCase() ?? "";
  const ua = navigator.userAgent.toLowerCase();
  const touchMac = ua.includes("macintosh") && navigator.maxTouchPoints > 1; // iPadOS reports as Mac
  if (nav.userAgentData?.mobile || /iphone|ipad|ipod|android/.test(ua) || touchMac) return "mobile";
  if (hint.includes("mac") || ua.includes("mac os")) return "mac";
  if (hint.includes("win") || ua.includes("windows")) return "windows";
  if (hint.includes("linux") || ua.includes("linux") || ua.includes("x11")) return "linux";
  return "mac";
}

/** Debian-family systems install a .deb with apt; browsers there often
 * say so in their user agent (Firefox on Ubuntu does). */
export function prefersDeb(): boolean {
  return /ubuntu|debian|linux mint|pop!_os/.test(navigator.userAgent.toLowerCase());
}

/** The device never changes while the page is open. */
export const subscribe = () => () => undefined;
