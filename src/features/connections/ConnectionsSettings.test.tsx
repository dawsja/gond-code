// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { check } from "../../../e2e/legibility/legibility";
import { ConnectionsSettings } from "./ConnectionsSettings";
import { MOCK_GITHUB } from "./mock";

afterEach(() => {
  cleanup();
  window.history.replaceState(null, "", "/");
});

it("a GitHub token never shows on screen", async () => {
  // ?demo and ?settings are read when the modules load.
  window.history.replaceState(null, "", "/?demo&settings=connections");
  vi.resetModules();
  const { Deck } = await import("@/features/deck/Deck");
  const view = render(<Deck />);
  fireEvent.click(await screen.findByRole("button", { name: /Add account/ }));
  fireEvent.click(screen.getByRole("radio", { name: "Use a token" }));
  fireEvent.change(screen.getByLabelText("Personal access token"), {
    target: { value: "ghp_secret123" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Save token" }));
  // The dialog closes once github.login.token has answered.
  await waitFor(() => expect(screen.queryByLabelText("Personal access token")).toBeNull());
  // The token is never on screen anywhere, before or after (the event log keeps it redacted
  // for agents; there's no Timeline to show it in).
  expect(view.container.ownerDocument.body.innerHTML).not.toContain("ghp_secret123");
}, 20_000); // re-imports the whole app for ?demo

describe("the shapes", () => {
  const show = (props: Partial<React.ComponentProps<typeof ConnectionsSettings>> = {}) => {
    const onCommand = vi.fn();
    render(<ConnectionsSettings status={MOCK_GITHUB} onCommand={onCommand} {...props} />);
    return onCommand;
  };

  it("without gh, offers prereq.install for it and says why Add account waits", () => {
    const onCommand = show({ status: { ghInstalled: false, accounts: [] } });
    expect(check(document.body)).toEqual([]);
    expect(screen.getByRole("button", { name: /Add account/ })).toHaveProperty("disabled", true);
    fireEvent.click(screen.getByRole("button", { name: "Install GitHub CLI" }));
    expect(onCommand).toHaveBeenCalledWith("prereq.install", { prereqs: ["gh"] });
  });

  it("shows loginError as a plain sentence", () => {
    show({ status: { ...MOCK_GITHUB, loginError: "The code expired before GitHub approved it." } });
    expect(screen.getByRole("alert").textContent).toBe(
      "The code expired before GitHub approved it.",
    );
  });

  it("github.login.web failing with gh_missing offers the install in the dialog", async () => {
    const onCommand = show({
      startWebLogin: () => Promise.reject({ code: "gh_missing", message: "gh not found" }),
    });
    fireEvent.click(screen.getByRole("button", { name: /Add account/ }));
    const dialog = await screen.findByRole("dialog");
    const alert = await within(dialog).findByRole("alert");
    expect(alert.textContent).toContain("The GitHub CLI isn't on this PC yet.");
    fireEvent.click(within(alert).getByRole("button", { name: "Install GitHub CLI" }));
    expect(onCommand).toHaveBeenCalledWith("prereq.install", { prereqs: ["gh"] });
  });

  it("a pending sign-in shows its code; a failed token keeps the dialog open with the reason", async () => {
    show({
      status: {
        ...MOCK_GITHUB,
        pending: {
          host: "github.com",
          userCode: "ABCD-1234",
          verificationUri: "https://github.com/login/device",
          expiresAt: new Date(Date.now() + 600_000).toISOString(),
        },
      },
      onToken: () =>
        Promise.reject({
          code: "github_failed",
          message: "That token doesn't have the repo scope.",
        }),
    });
    fireEvent.click(screen.getByRole("button", { name: /Add account/ }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("ABCD-1234")).toBeTruthy();
    fireEvent.click(within(dialog).getByRole("radio", { name: "Use a token" }));
    fireEvent.change(within(dialog).getByLabelText("Personal access token"), {
      target: { value: "ghp_x" },
    });
    fireEvent.click(within(dialog).getByRole("button", { name: "Save token" }));
    expect((await within(dialog).findByRole("alert")).textContent).toContain(
      "That token doesn't have the repo scope.",
    );
    expect(screen.getByRole("dialog")).toBeTruthy();
  });

  it("github_failed on the browser sign-in says why and retries", async () => {
    const start = vi
      .fn()
      .mockRejectedValueOnce({ code: "github_failed", message: "GitHub didn't answer in time." })
      .mockResolvedValueOnce({
        userCode: "WXYZ-0001",
        verificationUri: "https://github.com/login/device",
        expiresAt: new Date(Date.now() + 600_000).toISOString(),
      });
    show({ startWebLogin: start });
    fireEvent.click(screen.getByRole("button", { name: /Add account/ }));
    const dialog = await screen.findByRole("dialog");
    const alert = await within(dialog).findByRole("alert");
    expect(alert.textContent).toContain("GitHub didn't answer in time.");
    fireEvent.click(within(alert).getByRole("button", { name: "Retry" }));
    expect(await within(dialog).findByText("WXYZ-0001")).toBeTruthy();
    expect(start).toHaveBeenCalledTimes(2);
  });
});
