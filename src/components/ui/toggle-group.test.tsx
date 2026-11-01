// @vitest-environment happy-dom
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { ToggleGroup, ToggleGroupItem } from "./toggle-group";

afterEach(cleanup);

// A highlight inside a pill is a pill. Joined-button radius rules
// (rounded-none in the middle, rounded-l-lg at the ends) must never reach a track's options.
it("a track's options are pills at the ends and in the middle, at any spacing", () => {
  render(
    <ToggleGroup type="single" track spacing={0} value="ask" aria-label="Decision">
      <ToggleGroupItem value="auto">Auto</ToggleGroupItem>
      <ToggleGroupItem value="ask">Ask</ToggleGroupItem>
      <ToggleGroupItem value="deny">Deny</ToggleGroupItem>
    </ToggleGroup>,
  );
  const track = document.querySelector("[data-slot=toggle-group]")!;
  expect(track.className).toMatch(/\brounded-full\b/);
  for (const name of ["Auto", "Ask", "Deny"]) {
    const cls = screen.getByRole("radio", { name }).className;
    expect(cls).toMatch(/\brounded-full\b/);
    expect(cls).not.toMatch(/rounded-none|rounded-l-lg|rounded-r-lg|\brounded-lg\b/);
  }
});
