// @vitest-environment happy-dom
import { useEffect } from "react";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { MOCK_HARNESSES, MOCK_KINDS } from "@/features/harness/mock";
import { check } from "../../../e2e/legibility/legibility";
import { AgentAccountButtons, type AccountFlows } from "./AgentAccountButtons";

vi.mock("@/features/terminal/XtermPane", () => ({
  default: function FakeXterm({
    open,
  }: {
    open: (s: { cols: number; rows: number }) => Promise<unknown>;
  }) {
    useEffect(() => void open({ cols: 100, rows: 30 }), [open]);
    return <div data-testid="terminal" />;
  },
}));

afterEach(cleanup);

const flows = (over: Partial<AccountFlows> = {}): AccountFlows => ({
  harnesses: MOCK_HARNESSES,
  kindsOf: (h) => MOCK_KINDS[h],
  onCommand: vi.fn(),
  addSignIn: vi.fn().mockResolvedValue("acc_new"),
  startSignIn: vi.fn().mockResolvedValue({ ptyId: "pty_1" }),
  ...over,
});
const harness = (id: string) => MOCK_HARNESSES.find((h) => h.id === id)!;

it("an installed agent's card offers an API key (preset to it) and its own sign-in", async () => {
  const f = flows();
  render(<AgentAccountButtons harness={harness("claude")} flows={f} />);
  expect(check(document.body)).toEqual([]);
  fireEvent.click(screen.getByRole("button", { name: /Use an API key/ }));
  const d = screen.getByRole("dialog");
  expect(
    within(d)
      .getByRole("radio", { name: /Claude Code/ })
      .getAttribute("aria-checked"),
  ).toBe("true");
  expect(within(d).getByRole("radio", { name: "API key" }).getAttribute("aria-checked")).toBe(
    "true",
  );
  fireEvent.keyDown(d, { key: "Escape" });
  fireEvent.click(screen.getByRole("button", { name: /Sign in/ }));
  await waitFor(() =>
    expect(f.addSignIn).toHaveBeenCalledWith({
      harness: "claude",
      kind: "oauth",
      label: "Claude Code sign-in",
    }),
  );
  await screen.findByTestId("terminal");
  expect(f.startSignIn).toHaveBeenCalledWith("acc_new", { cols: 100, rows: 30 });
});

it("shows only what the harness takes; before the install, a key but not its own sign-in", () => {
  const { container } = render(
    <AgentAccountButtons harness={{ ...harness("codex"), installed: false }} flows={flows()} />,
  );
  expect(container.textContent).toContain("Use an API key");
  expect(screen.queryByRole("button", { name: /Sign in/ })).toBeNull();
  cleanup();
  render(
    <AgentAccountButtons
      harness={harness("claude")}
      flows={flows({ kindsOf: () => ({ provider: "google", kinds: ["oauth"] }) })}
    />,
  );
  expect(screen.queryByRole("button", { name: /Use an API key/ })).toBeNull();
  expect(screen.getByRole("button", { name: /Sign in/ })).toBeTruthy();
});
