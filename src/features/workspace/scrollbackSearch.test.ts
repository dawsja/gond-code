import { describe, expect, it } from "vitest";
import { searchLines } from "./scrollbackSearch";

describe("searchLines", () => {
  const lines = [
    { row: 0, text: String.raw`PS C:\code> cargo test` },
    { row: 1, text: "error[E0433]: cannot find `select` in `tokio`" },
    // A logical line wrapped over rows 2 and 3 at 20 columns.
    { row: 2, text: "warning: unused variable `Error` here" },
    { row: 4, text: "test result: FAILED. 1 error" },
  ];

  it("finds every match, newest first, case-insensitive, with where it is", () => {
    const hits = searchLines(lines, 20, "error", 10);
    // "error" at 23 of the row-4 line (row 5, col 3 at 20 columns); "Error" at 26 of the
    // wrapped row-2 line (row 3, col 6); at 0 of row 1.
    expect(hits.map((h) => [h.row, h.col])).toEqual([
      [5, 3],
      [3, 6],
      [1, 0],
    ]);
    expect(hits[1]).toMatchObject({ match: "Error", before: "warning: unused variable `" });
  });

  it("stops at the limit and ignores an empty query", () => {
    expect(searchLines(lines, 20, "e", 2)).toHaveLength(2);
    expect(searchLines(lines, 20, "  ", 5)).toEqual([]);
    expect(searchLines(lines, 20, "nothing like this", 5)).toEqual([]);
  });
});
