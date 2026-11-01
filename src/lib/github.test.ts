import { describe, expect, it } from "vitest";
import { addedLine, errorSentence, expiresIn, initialsOf, statusOf } from "./github";

const now = Date.parse("2026-10-30T12:00:00Z");
const day = 86_400_000;

describe("github", () => {
  it("reads the card's status from gh", () => {
    expect(statusOf({ ghInstalled: false, accounts: [] }).label).toBe("GitHub CLI missing");
    expect(statusOf({ ghInstalled: true, accounts: [] }).label).toBe("Not signed in");
  });

  it("says how and when an account was added", () => {
    const a = {
      login: "sam",
      host: "github.com",
      active: true,
      scopes: [],
      addedAt: new Date(now - 3 * day).toISOString(),
    };
    expect(addedLine({ ...a, method: "web" }, now)).toBe("Signed in with browser · 3 days ago");
    expect(addedLine({ ...a, method: "token" }, now)).toBe("Token · added 3 days ago");
    expect(addedLine({ ...a, method: undefined, addedAt: undefined }, now)).toBe(
      "Already signed in with GitHub CLI",
    );
  });

  it("turns gh_missing and github_failed into plain sentences", () => {
    expect(errorSentence({ code: "gh_missing", message: "gh not found" })).toEqual({
      text: "The GitHub CLI isn't on this PC yet. Install it, then sign in.",
      ghMissing: true,
    });
    expect(
      errorSentence({ code: "github_failed", message: "That token doesn't have the repo scope." }),
    ).toEqual({ text: "That token doesn't have the repo scope.", ghMissing: false });
    expect(errorSentence(undefined).text).toBe("GitHub didn't finish signing you in. Try again.");
  });

  it("gives accounts that share a first letter different initials", () => {
    expect(initialsOf("sam")).toBe("S");
    expect(initialsOf("sam-work")).toBe("SW");
  });

  it("counts a device code down in mm:ss", () => {
    expect(expiresIn(new Date(now + 872_000).toISOString(), now)).toBe("14:32");
    expect(expiresIn(new Date(now - 1000).toISOString(), now)).toBe("0:00");
  });
});
