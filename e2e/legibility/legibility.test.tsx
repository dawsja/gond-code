// @vitest-environment happy-dom
// renders the app on the in-memory mock and checks every interactive element against the
// legibility rule (legibility.ts). The app starts with known debt, counted in baseline.json:
// a PR may only lower that count, never raise it, and must lower the baseline when it fixes some.
import { act, cleanup, render } from "@testing-library/react";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { Deck } from "@/features/deck/Deck";
import { check } from "./legibility";

const BASELINE = join(import.meta.dirname, "baseline.json");

afterEach(() => {
  cleanup();
  document.body.innerHTML = "";
});

function problemsIn(html: string) {
  document.body.innerHTML = html;
  return check(document.body);
}

describe("the legibility rule", () => {
  it("passes a labelled command button, a UI-only control and an entity", () => {
    expect(
      problemsIn(`
        <button data-command="thread.create">New thread</button>
        <button data-local="copy-id" aria-label="Copy id"><svg></svg></button>
        <div role="option" data-entity="thread" data-id="t1">Fix CI</div>
        <button data-command="ship.push" disabled title="Not available yet" aria-description="Not available yet">Push</button>`),
    ).toEqual([]);
  });

  it("fails an unlabelled button, an icon button without a name, and a disabled one without a reason", () => {
    const problems = problemsIn(`
      <button>Save</button>
      <button data-command="settings.open"><svg></svg></button>
      <div tabindex="0" data-local="focus">Panel</div>
      <button data-command="ship.push" disabled>Push</button>`);
    expect(problems.map((p) => p.missing)).toEqual([
      ["data-command | data-local | data-entity+data-id"],
      ["accessible name"],
      ["role"],
      ["disabled reason (title + aria-description)"],
    ]);
  });

  it("an entity needs both data-entity and data-id", () => {
    expect(problemsIn(`<button data-entity="thread">Fix CI</button>`)).toHaveLength(1);
  });
});

// The app is the Deck, on the mock backend: crew, board and inbox. (Its dev
// flags, ?demo and ?bar, are read once at load, so they can't be switched per screen here.)
const SCREENS = [""];

async function problemsOnScreen(search: string) {
  window.history.replaceState(null, "", `/${search}`);
  const view = render(<Deck />);
  // Let the mock's async calls (projects, threads, settings) settle and re-render.
  for (let i = 0; i < 10; i++) await act(() => new Promise((r) => setTimeout(r, 20)));
  const problems = check(view.container.ownerDocument.body).map((p) => ({
    ...p,
    screen: search || "main",
  }));
  cleanup();
  return problems;
}

it("the app has no new controls an agent can't read", async () => {
  const problems = [];
  for (const screen of SCREENS) problems.push(...(await problemsOnScreen(screen)));
  const list = problems
    .map((p) => `  [${p.screen}] ${p.key}: missing ${p.missing.join(", ")}`)
    .join("\n");

  if (process.env.UPDATE_LEGIBILITY_BASELINE)
    writeFileSync(BASELINE, JSON.stringify({ unlabelled: problems.length }, null, 2) + "\n");
  const { unlabelled } = JSON.parse(readFileSync(BASELINE, "utf8")) as { unlabelled: number };

  expect(
    problems.length,
    `${problems.length} controls an agent can't read (baseline ${unlabelled}); label new ones per e2e/legibility/legibility.ts:\n${list}`,
  ).toBeLessThanOrEqual(unlabelled);
  expect(
    problems.length,
    `fixed some, nice: lower "unlabelled" in e2e/legibility/baseline.json to ${problems.length}`,
  ).toBe(unlabelled);
});

// a one-line field that truncates, or a title that clamps, shows its full value in
// a tooltip (a shadcn one through Hint: data-hint), so a person can read it and an agent finds it
// in the DOM.
it("every truncated or clamped text has a tooltip", async () => {
  window.history.replaceState(null, "", "/");
  const view = render(<Deck />);
  for (let i = 0; i < 10; i++) await act(() => new Promise((r) => setTimeout(r, 20)));
  const cut = [
    ...view.container.ownerDocument.body.querySelectorAll<HTMLElement>(
      '.truncate, [class*="line-clamp-"]',
    ),
  ].filter((el) => (el.textContent ?? "").trim() && !el.closest("[title], [data-hint]"));
  const list = cut.map((el) => `  <${el.tagName.toLowerCase()}> “${el.textContent?.trim()}”`);
  cleanup();
  expect(cut.length, `truncated text without a tooltip:\n${list.join("\n")}`).toBe(0);
});
