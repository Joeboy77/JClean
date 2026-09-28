// Stands in for the engine until phase 3: streams mock results into the
// store the way real scan events will, and pretends to clean.

import { mockItems, mockRowTarget } from "../data/mock";
import { useStore } from "./store";

let timer: number | undefined;

function stop() {
  if (timer !== undefined) {
    window.clearInterval(timer);
    timer = undefined;
  }
}

export function startScan() {
  stop();
  const store = useStore.getState();
  store.beginScan();
  const target = mockRowTarget();
  const all = mockItems(target ? Math.ceil(target / 2.75) : 0);
  const batches = 24;
  const perBatch = Math.ceil(all.length / batches);
  let sent = 0;
  // A short pause first, so the skeleton state is visible like on a real scan.
  let tick = -3;
  timer = window.setInterval(() => {
    tick++;
    const s = useStore.getState();
    if (tick <= 0) {
      s.setProgress(0.02 * (tick + 4));
      return;
    }
    s.addItems(all.slice(sent, sent + perBatch));
    sent += perBatch;
    s.setProgress(Math.min(1, sent / all.length));
    if (sent >= all.length) {
      stop();
      s.finishScan(false);
    }
  }, 100);
}

/** Stops the scan and keeps what was found so far. */
export function cancelScan() {
  stop();
  useStore.getState().finishScan(true);
}

export function startClean() {
  stop();
  const store = useStore.getState();
  const ids = [...store.selected];
  const freed = store.items
    .filter((i) => store.selected.has(i.id))
    .reduce((n, i) => n + i.bytes, 0);
  store.beginClean();
  let step = 0;
  timer = window.setInterval(() => {
    step++;
    useStore.getState().setProgress(step / 12);
    if (step >= 12) {
      stop();
      useStore.getState().finishClean(ids, freed);
    }
  }, 100);
}
