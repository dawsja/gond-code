import { describe, expect, it } from "vitest";
import { findFileRefs } from "./fileLinks";

const refs = (s: string) =>
  findFileRefs(s).map(({ path, line, column }) => ({ path, line, column }));

describe("findFileRefs", () => {
  it("finds paths with a folder or a line number", () => {
    expect(refs(String.raw`error[E0433]: --> src\pty\host.rs:199:16`)).toEqual([
      { path: String.raw`src\pty\host.rs`, line: 199, column: 16 },
    ]);
    expect(refs("  at render (src/features/Deck.tsx:42)")).toEqual([
      { path: "src/features/Deck.tsx", line: 42, column: undefined },
    ]);
    expect(refs(String.raw`Edited C:\code\gond\README.md and ./docs/PRODUCT.md`)).toEqual([
      { path: String.raw`C:\code\gond\README.md`, line: undefined, column: undefined },
      { path: "./docs/PRODUCT.md", line: undefined, column: undefined },
    ]);
    expect(refs("main.c(12,5): warning")).toEqual([{ path: "main.c", line: 12, column: 5 }]);
    expect(refs("lib.rs:7")).toEqual([{ path: "lib.rs", line: 7, column: undefined }]);
  });

  it("leaves words, versions and URLs alone", () => {
    expect(refs("e.g. Node.js and Claude Code v2.1.316")).toEqual([]);
    expect(refs("see https://github.com/dawsja/gond-code/blob/main/README.md")).toEqual([]);
  });

  it("says where in the line each one is", () => {
    const [r] = findFileRefs("--> src/a.rs:3");
    expect(r.index).toBe(4);
    expect(r.length).toBe("src/a.rs:3".length);
  });
});
