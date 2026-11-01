import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { expect, it } from "vitest";

// Nothing ships greyed out as "Not available yet": what can't run is hidden until its command is
// registered.
const MINE = ["deck", "settings", "shell", "workspace"];
const files = (dir: string): string[] =>
  readdirSync(dir).flatMap((f) => {
    const p = join(dir, f);
    return statSync(p).isDirectory()
      ? files(p)
      : /\.tsx?$/.test(f) && !/\.test\./.test(f)
        ? [p]
        : [];
  });

it("no Not available yet stubs in the app's views", () => {
  const hits = MINE.flatMap((d) => files(join(__dirname, d))).filter((f) =>
    /Not available yet/.test(readFileSync(f, "utf8")),
  );
  expect(hits).toEqual([]);
});
