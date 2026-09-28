import { beforeEach, describe, expect, it } from "vitest";
import type { StorageItem } from "../data/types";
import { useStore } from "./store";

function item(id: string, preselected = true): StorageItem {
  return {
    id,
    ruleId: "macos.node.npm-cache",
    name: null,
    path: `/Users/me/${id}`,
    bytes: 1000,
    bytesKnown: true,
    lastUsed: null,
    risk: "safe",
    category: "developer",
    method: "delete",
    cleanable: true,
    blockedReason: null,
    preselected,
    mayShareBlocks: false,
    project: null,
  };
}

describe("cleaning state", () => {
  beforeEach(() => {
    const s = useStore.getState();
    s.beginScan("quick");
    s.addItems([item("a"), item("b"), item("c", false)]);
    s.finishScan({ partial: false });
  });

  it("pre-selects safe unused items as they arrive", () => {
    expect([...useStore.getState().selected].sort()).toEqual(["a", "b"]);
  });

  it("removes cleaned items as outcomes arrive and keeps failed ones", () => {
    const s = useStore.getState();
    s.startCleaning(2);
    s.recordOutcome({
      itemId: "a",
      label: "A",
      outcome: "cleaned",
      bytes: 1000,
      reason: null,
      method: "delete",
    });
    s.recordOutcome({
      itemId: "b",
      label: "B",
      outcome: "failed",
      bytes: 0,
      reason: "No permission",
      method: "delete",
    });
    const after = useStore.getState();
    expect(after.items.map((i) => i.id)).toEqual(["b", "c"]);
    expect(after.selected.has("a")).toBe(false);
    expect(after.progress).toBe(1);
    after.finishCleaning({
      cleanedBytes: 1000,
      trashedBytes: 0,
      measuredFreed: 1000,
      failed: 1,
      skipped: 0,
    });
    expect(useStore.getState().phase).toBe("done");
    expect(useStore.getState().freed).toBe(1000);
  });

  it("counts Trash moves as not yet freed", () => {
    const s = useStore.getState();
    s.startCleaning(1);
    s.finishCleaning({
      cleanedBytes: 3000,
      trashedBytes: 1000,
      measuredFreed: null,
      failed: 0,
      skipped: 0,
    });
    expect(useStore.getState().freed).toBe(2000);
  });
});
