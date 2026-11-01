import { expect, it } from "vitest";
import { windowLine } from "./usage";

it("a plan window reads as used, with when it resets", () => {
  const now = new Date("2026-10-30T12:00:00");
  const l = windowLine(
    { name: "session", percentLeft: 94.4, resetsAt: "2026-10-30T15:40:00" },
    now,
  );
  expect(l.label).toBe("Session");
  expect(l.text).toBe("6% used");
  expect(l.resets).toMatch(/^resets .*3:40/);
  expect(windowLine({ name: "weekly", percentLeft: 53 }, now)).toMatchObject({
    label: "Weekly",
    text: "47% used",
    resets: undefined,
  });
  expect(windowLine({ name: "weekly", percentLeft: -5 }, now)).toMatchObject({
    left: 0,
    used: 100,
  });
});
