// @vitest-environment happy-dom
import { useEffect } from "react";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";

// xterm needs a canvas; the stub starts the pty the way XtermPane does.
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

it("a new sign-in account is added, then its sign-in opens", async () => {
  const { AddAccount } = await import("./AddAccount");
  const { MOCK_HARNESSES, MOCK_KINDS } = await import("@/features/harness/mock");
  const signIn = vi.fn().mockResolvedValue(undefined);
  const onClose = vi.fn();
  render(
    <AddAccount
      harnesses={MOCK_HARNESSES}
      kindsOf={(h) => MOCK_KINDS[h]}
      onCommand={vi.fn()}
      onClose={onClose}
      signIn={signIn}
    />,
  );
  const d = screen.getByRole("dialog");
  fireEvent.click(within(d).getByRole("radio", { name: "Sign in" }));
  const go = within(d).getByRole("button", { name: "Sign in" });
  expect(go).toHaveProperty("disabled", true);
  fireEvent.change(within(d).getByRole("textbox", { name: "Label" }), {
    target: { value: "Max plan" },
  });
  fireEvent.click(go);
  expect(signIn).toHaveBeenCalledWith({ harness: "claude", kind: "oauth", label: "Max plan" });
  expect(onClose).toHaveBeenCalled();
});
