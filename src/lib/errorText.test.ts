import { describe, expect, it } from "vitest";
import { errorText } from "./types";

describe("errorText", () => {
  it("shows the message of an app error, an Error, or any object with a message", () => {
    expect(errorText({ code: "invalid", message: "Codex isn't installed" })).toBe(
      "Codex isn't installed",
    );
    expect(errorText(new Error("boom"))).toBe("boom");
    expect(errorText({ message: "plain" })).toBe("plain");
  });
  it("never shows [object Object]", () => {
    expect(errorText({ nope: 1 })).toBe('{"nope":1}');
    expect(errorText("text")).toBe("text");
    expect(errorText(undefined)).not.toContain("[object");
  });
});
