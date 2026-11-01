import { expect, it } from "vitest";
import type { Account } from "@/lib/harnesses";
import { accountLine } from "./accountLine";

it("an account reads as the sign-in you'd recognise, never its organisation id", () => {
  const own = {
    label: "me@example.com · Claude Max",
    identity: "me@example.com/d006043d-fdf4-47d2-a1b2-c3d4e5f6a7b8",
  } as Account;
  expect(accountLine(own)).toBe("me@example.com · Claude Max");
  expect(
    accountLine({ label: "Codex sign-in", identity: "me@example.com/9daae127" } as Account),
  ).toBe("me@example.com");
  expect(accountLine({ label: "Work", plan: "Max plan" } as Account)).toBe("Work · Max plan");
  expect(accountLine(undefined)).toBe("Its own sign-in");
});
