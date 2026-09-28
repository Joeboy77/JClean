// Dev-only: collects React commit times for the map (see demo.ts, "fps").
export const commits: { phase: string; ms: number }[] = [];

export function onRender(_id: string, phase: string, actualDuration: number) {
  commits.push({ phase, ms: actualDuration });
}
