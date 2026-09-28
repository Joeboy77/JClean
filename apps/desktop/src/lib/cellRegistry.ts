// DOM elements of the map cells currently on screen, so the connector line
// can find the cell a hovered row points at.

const cells = new Map<string, HTMLElement>();

export function registerCell(id: string, el: HTMLElement | null) {
  if (el) cells.set(id, el);
  else cells.delete(id);
}

export function cellElement(id: string): HTMLElement | undefined {
  return cells.get(id);
}
