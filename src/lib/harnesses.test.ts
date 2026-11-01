import { describe, expect, it } from "vitest";
import { fromModelList } from "./harnesses";

describe("model.list", () => {
  it("maps model.list to the pickers: no levels, no control; an unknown default is the middle", () => {
    const [opus, haiku, gpt] = fromModelList([
      {
        id: "opus",
        label: "Opus 5.5",
        provider: "claude",
        thinking: { levels: ["low", "medium", "high", "max"], default: "high", via: "flag" },
      },
      {
        id: "haiku",
        label: "Haiku 5.5",
        provider: "claude",
        thinking: { levels: [], via: "flag" },
      },
      {
        id: "gpt",
        label: "GPT-5.5",
        provider: "openai",
        thinking: { levels: ["minimal", "low", "medium", "high"], default: null, via: "flag" },
      },
    ]);
    expect(opus).toMatchObject({ isDefault: true, thinking: { default: "high", how: "flag" } });
    expect(haiku.thinking).toBeNull();
    expect(gpt.thinking?.default).toBe("low");
  });
});
