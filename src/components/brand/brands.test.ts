import { describe, expect, it } from "vitest";
import { BRANDS, brandOf, initials } from "./brands";
import { COLOR, MONO } from "./svgs";

describe("brand icons", () => {
  it("has a real LobeHub SVG for every id, in the variant it asks for", () => {
    for (const [id, brand] of Object.entries(BRANDS)) {
      expect(MONO[brand.icon], `${id}: mono ${brand.icon}`).toMatch(/^<svg[\s>]/);
      if (brand.color)
        expect(COLOR[brand.icon], `${id}: color ${brand.icon}`).toMatch(/^<svg[\s>]/);
    }
  });

  it("draws black-by-default logos in currentColor, so they show on near-black", () => {
    for (const id of ["openai", "xai", "grok", "ollama", "cursor", "opencode", "github"]) {
      const brand = brandOf(id)!;
      expect(brand.color, id).toBe(false);
      expect(MONO[brand.icon], id).toContain('fill="currentColor"');
    }
  });

  it("knows the ids Rust sends and falls back to initials otherwise", () => {
    expect(brandOf("claude")?.name).toBe("Claude");
    expect(brandOf("Codex")?.name).toBe("Codex");
    expect(brandOf("claude-code")?.icon).toBe("claude");
    expect(brandOf("aider")).toBeUndefined();
    expect(initials("My agent")).toBe("MA");
    expect(brandOf("custom")).toBeUndefined(); // #86: custom agents → initials of their name
    expect(initials("aider")).toBe("AI");
  });
});
