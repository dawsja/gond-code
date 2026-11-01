// Search across every pane's scrollback (Ctrl+Shift+F): each terminal hands over its lines
// (wrapped lines joined back into one), and this finds the query in them, newest first.

/** A logical line: its text and the buffer row it starts on. */
export type Line = { row: number; text: string };

/** A match: where it is in the buffer (to scroll to and select), and the text around it. */
export type Hit = {
  row: number;
  col: number;
  len: number;
  before: string;
  match: string;
  after: string;
};

/**
 * Up to `limit` matches of `query` (case-insensitive) in `lines`, the newest (lowest on screen)
 * first. `cols` maps a position in a wrapped line back to its row and column.
 */
export function searchLines(lines: Line[], cols: number, query: string, limit: number): Hit[] {
  const q = query.trim().toLowerCase();
  if (!q || limit <= 0) return [];
  const hits: Hit[] = [];
  for (let i = lines.length - 1; i >= 0 && hits.length < limit; i--) {
    const { row, text } = lines[i];
    const lower = text.toLowerCase();
    const at: number[] = [];
    for (let k = lower.indexOf(q); k >= 0; k = lower.indexOf(q, k + q.length)) at.push(k);
    for (const k of at.reverse()) {
      if (hits.length >= limit) break;
      hits.push({
        row: row + Math.floor(k / cols),
        col: k % cols,
        len: q.length,
        before: text.slice(Math.max(0, k - 48), k).trimStart(),
        match: text.slice(k, k + q.length),
        after: text.slice(k + q.length, k + q.length + 96).trimEnd(),
      });
    }
  }
  return hits;
}

const SHOW = "gond:search";

/** Opens search across every pane (Ctrl+Shift+F, wherever focus is). */
export const showSearch = () => window.dispatchEvent(new Event(SHOW));

/** Calls `f` whenever search is asked for. */
export function onShowSearch(f: () => void) {
  window.addEventListener(SHOW, f);
  return () => window.removeEventListener(SHOW, f);
}
