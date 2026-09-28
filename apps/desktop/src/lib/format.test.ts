import { describe, expect, it } from "vitest";
import { basename, formatAgo, formatBytes, speakBytes, tildify } from "./format";

describe("formatBytes", () => {
  it("uses decimal units with one decimal from 1 GB", () => {
    expect(formatBytes(0)).toBe("0 bytes");
    expect(formatBytes(12_400)).toBe("12 KB");
    expect(formatBytes(824_000_000)).toBe("824 MB");
    expect(formatBytes(6_120_000_000)).toBe("6.1 GB");
    expect(formatBytes(38_650_000_000)).toBe("38.7 GB");
  });

  it("speaks units in full", () => {
    expect(speakBytes(6_100_000_000)).toBe("6.1 gigabytes");
  });
});

describe("formatAgo", () => {
  const now = 1_800_000_000;
  it("reads naturally", () => {
    expect(formatAgo(now, now)).toBe("today");
    expect(formatAgo(now - 86_400, now)).toBe("yesterday");
    expect(formatAgo(now - 10 * 86_400, now)).toBe("10 days ago");
    expect(formatAgo(now - 210 * 86_400, now)).toBe("7 months ago");
    expect(formatAgo(now - 800 * 86_400, now)).toBe("2 years ago");
  });
});

describe("paths", () => {
  it("shortens the home folder", () => {
    expect(tildify("/Users/me/.npm", "/Users/me")).toBe("~/.npm");
    expect(tildify("/Users/meh", "/Users/me")).toBe("/Users/meh");
    expect(basename("/a/b/node_modules/")).toBe("node_modules");
  });
});
