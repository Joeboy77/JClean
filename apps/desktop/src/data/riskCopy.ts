import type { Risk } from "./types";

/** What each risk level means, in plain language (spec §5.6). */
export const RISK_COPY: Record<Risk, { title: string; short: string; why: string }> = {
  safe: {
    title: "Safe to clean",
    short: "Safe",
    why: "These come back on their own when needed. Cleaning them doesn't lose any of your data.",
  },
  review: {
    title: "Needs review",
    short: "Review",
    why: "Probably fine to clean, but take a look first. Some may hold things you want to keep.",
  },
  caution: {
    title: "Caution",
    short: "Caution",
    why: "May contain data that can't be recovered. JClean asks you to confirm again before cleaning these.",
  },
  info: {
    title: "For your information",
    short: "Info",
    why: "Shown so you can see where your space goes. JClean doesn't clean these.",
  },
};
