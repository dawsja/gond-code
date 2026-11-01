// @vitest-environment happy-dom
//  / account.signIn runs in the background; /signins/{jobId} drives the card.
import { useState } from "react";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { check } from "../../../e2e/legibility/legibility";
import type { SignInJob } from "./SignIn";

const tree: { signins?: Record<string, SignInJob> } = {};
let listeners: (() => void)[] = [];
vi.mock("@/lib/appState", () => ({
  runCommand: vi.fn(),
  useAppState: (select: (t: typeof tree) => unknown) => {
    const [, bump] = useState(0);
    listeners.push(() => bump((n) => n + 1));
    return select(tree);
  },
}));
vi.mock("@/features/terminal/XtermPane", () => ({
  default: () => <div data-testid="terminal" />,
}));
const job = (j: Partial<SignInJob>) =>
  act(() => {
    tree.signins = { sgn_1: { id: "sgn_1", status: "starting", ...j } };
    listeners.forEach((l) => l());
  });
afterEach(() => {
  cleanup();
  listeners = [];
  tree.signins = undefined;
});

async function show() {
  const { SignIn } = await import("./SignIn");
  const actions = {
    sendCode: vi.fn().mockResolvedValue(null),
    cancel: vi.fn().mockResolvedValue(null),
  };
  const start = vi.fn().mockResolvedValue({ ptyId: "pty_1", jobId: "sgn_1" });
  const onClose = vi.fn();
  render(
    <SignIn label="Max" harness="Claude Code" start={start} onClose={onClose} actions={actions} />,
  );
  await act(async () => {});
  return { actions, start, onClose };
}

it("shows the code and the sign-in page in the app, with no terminal until Show details", async () => {
  await show();
  job({ status: "waiting", url: "https://claude.ai/oauth", code: "WXYZ-1234" });
  expect(screen.getByText("Approve in your browser… Waiting")).toBeTruthy();
  expect(screen.getByText("WXYZ-1234")).toBeTruthy();
  expect(screen.getByRole("button", { name: "Open the page again" })).toBeTruthy();
  expect(screen.queryByTestId("terminal")).toBeNull();
  expect(check(document.body)).toEqual([]);
  fireEvent.click(screen.getByRole("button", { name: "Show details" }));
  expect(await screen.findByTestId("terminal")).toBeTruthy();
});

it("the code box waits behind Got a code?, even on needsCode, and sends account.signIn.code", async () => {
  const { actions } = await show();
  job({ status: "needsCode" });
  expect(screen.queryByRole("textbox")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Got a code?" }));
  fireEvent.change(screen.getByRole("textbox", { name: "Code from your browser" }), {
    target: { value: " abc123 " },
  });
  expect(check(document.body)).toEqual([]);
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "Send code" })));
  expect(actions.sendCode).toHaveBeenCalledWith("sgn_1", "abc123");
});

it("Cancel stops the sign-in; a failure says why and offers Retry; signed in says Done", async () => {
  const { actions, start, onClose } = await show();
  job({ status: "waiting" });
  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  expect(actions.cancel).toHaveBeenCalledWith("sgn_1");
  expect(onClose).toHaveBeenCalled();

  job({ status: "failed", message: "The sign-in timed out after 10 minutes." });
  expect(screen.getByRole("alert").textContent).toContain("didn't finish");
  expect(screen.getByText("The sign-in timed out after 10 minutes.")).toBeTruthy();
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "Retry" })));
  await waitFor(() => expect(start).toHaveBeenCalledTimes(2));

  job({ status: "signedIn", message: "Signed in as sam@example.com" });
  expect(screen.getByRole("button", { name: "Done" })).toBeTruthy();
});

it("Show details shows the login's own text when the job carries it; a failed start offers Retry", async () => {
  await show();
  job({
    status: "failed",
    message: "No sign-in link after 20 s.",
    output: "Opening browser…\nerror: no display",
  });
  expect(screen.getByText("No sign-in link after 20 s.")).toBeTruthy();
  expect(screen.getByRole("button", { name: "Retry" })).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Show details" }));
  expect(screen.getByLabelText("Sign-in details").textContent).toContain("error: no display");
  expect(screen.queryByTestId("terminal")).toBeNull();
});
