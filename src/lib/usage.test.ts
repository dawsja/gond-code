import { describe, expect, it } from "vitest";
import { tightest, type PlanUsage } from "./usage";

const now = new Date("2026-11-02T12:00:00Z");
const plan = (windows: PlanUsage["windows"]): PlanUsage => ({
  harness: "claude",
  windows,
  updatedAt: now.toISOString(),
});

describe("tightest", () => {
  it("picks the window with the least left, and says how close it is", () => {
    const t = tightest(
      plan([
        { name: "session", percentLeft: 70, resetsAt: "2026-11-02T15:00:00Z" },
        { name: "weekly", percentLeft: 8, resetsAt: "2026-11-06T09:00:00Z" },
      ]),
      now,
    );
    expect(t).toMatchObject({ name: "weekly", left: 8, level: "low" });
    expect(t?.resets).toMatch(/^resets /);
  });

  it("ignores a window that has already reset, and marks a used-up one", () => {
    expect(
      tightest(
        plan([
          { name: "session", percentLeft: 0, resetsAt: "2026-11-02T11:00:00Z" },
          { name: "weekly", percentLeft: 40 },
        ]),
        now,
      ),
    ).toMatchObject({ name: "weekly", level: "ok" });
    expect(
      tightest(plan([{ name: "session", percentLeft: 0, resetsAt: "2026-11-02T13:00:00Z" }]), now)
        ?.level,
    ).toBe("out");
    expect(tightest(plan([]), now)).toBeUndefined();
    expect(tightest(undefined, now)).toBeUndefined();
  });
});
