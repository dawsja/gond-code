// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { MOCK_HARNESSES, MOCK_KINDS } from "@/features/harness/mock";
import { check } from "../../../e2e/legibility/legibility";
import { AddAccount } from "./AddAccount";

afterEach(cleanup);

function show() {
  const onCommand = vi.fn();
  const onClose = vi.fn();
  render(
    <AddAccount
      harnesses={MOCK_HARNESSES}
      kindsOf={(h) => MOCK_KINDS[h]}
      onCommand={onCommand}
      onClose={onClose}
      // Sign in and Test connection only show when they can run (no stubs).
      signIn={vi.fn().mockResolvedValue(undefined)}
      testDraft={vi.fn().mockResolvedValue({ ok: true, ms: 1, models: [] })}
    />,
  );
  const d = screen.getByRole("dialog");
  const box = (name: string) => within(d).getByRole("textbox", { name });
  return { onCommand, onClose, d, box };
}

it("is legible with every option open, and only offers what the harness takes", () => {
  const { d } = show();
  fireEvent.click(within(d).getByRole("button", { name: /More options/ }));
  fireEvent.click(within(d).getByRole("button", { name: "Add a header" }));
  expect(check(document.body)).toEqual([]);
  const kinds = within(d).getByRole("radiogroup", { name: "How it signs in" });
  expect(
    within(kinds)
      .getAllByRole("radio")
      .map((r) => r.textContent),
  ).toEqual(["Sign in", "API key", "Token"]);
  expect(within(d).getByRole("radiogroup", { name: "Where it runs" })).toBeTruthy();
  fireEvent.click(within(d).getByRole("radio", { name: /Codex/ }));
  expect(within(kinds).queryByRole("radio", { name: "Token" })).toBeNull();
  expect(within(d).queryByRole("radiogroup", { name: "Where it runs" })).toBeNull();
  expect(within(d).getByRole("textbox", { name: "Azure API version" })).toBeTruthy();
});

it("a Claude gateway: https or localhost base URL, Bearer and a header, in one account.add", () => {
  const { d, box, onCommand, onClose } = show();
  fireEvent.change(box("Label"), { target: { value: "LiteLLM" } });
  fireEvent.change(within(d).getByLabelText("API key", { selector: "input" }), {
    target: { value: "sk-test" },
  });
  const add = within(d).getByRole("button", { name: "Add account" });
  fireEvent.change(box("Base URL"), { target: { value: "http://gateway.internal" } });
  expect(add).toHaveProperty("disabled", true);
  fireEvent.change(box("Base URL"), { target: { value: "http://localhost:4000" } });
  expect(add).toHaveProperty("disabled", false);
  fireEvent.click(within(d).getByRole("button", { name: /More options/ }));
  fireEvent.click(within(d).getByRole("switch", { name: "Send key as Bearer" }));
  fireEvent.click(within(d).getByRole("button", { name: "Add a header" }));
  fireEvent.change(box("Header 1 name"), { target: { value: "Authorization" } });
  expect(within(d).getByText(/Auth headers come from the key/)).toBeTruthy();
  expect(add).toHaveProperty("disabled", true);
  fireEvent.change(box("Header 1 name"), { target: { value: "X-Team" } });
  fireEvent.change(box("Header 1 value"), { target: { value: "web" } });
  fireEvent.click(add);
  expect(onCommand).toHaveBeenCalledTimes(1);
  expect(onCommand).toHaveBeenCalledWith("account.add", {
    harness: "claude",
    kind: "apiKey",
    label: "LiteLLM",
    secret: "sk-test",
    endpoint: "http://localhost:4000",
    options: { bearer: true, headers: { "X-Team": "web" } },
  });
  expect(onClose).toHaveBeenCalled();
});

it("Vertex needs a region and project but no key", () => {
  const { d, box, onCommand } = show();
  fireEvent.change(box("Label"), { target: { value: "Vertex" } });
  fireEvent.click(within(d).getByRole("radio", { name: "Google Vertex" }));
  expect(within(d).queryByLabelText("API key", { selector: "input" })).toBeNull();
  expect(within(d).queryByRole("textbox", { name: "Base URL" })).toBeNull();
  fireEvent.change(box("Region"), { target: { value: "us-east5" } });
  fireEvent.change(box("Google Cloud project"), { target: { value: "acme-ai" } });
  fireEvent.click(within(d).getByRole("button", { name: "Add account" }));
  expect(onCommand).toHaveBeenCalledWith("account.add", {
    harness: "claude",
    kind: "apiKey",
    label: "Vertex",
    options: { cloud: { kind: "vertex", region: "us-east5", project: "acme-ai" } },
  });
});

it("Test connection says why it waits", () => {
  const { d } = show();
  const test = within(d).getByRole("button", { name: "Test connection" });
  expect(test).toHaveProperty("disabled", true);
  expect(test.closest("[data-hint]")?.getAttribute("data-hint")).toBe("A label first");
});

it("Test connection runs account.test on the draft and shows the models, or the reason", async () => {
  const testDraft = vi
    .fn()
    .mockResolvedValueOnce({ ok: true, ms: 340, models: ["claude-opus-5-5", "claude-haiku-5-5"] })
    .mockResolvedValueOnce({ ok: false, error: "The endpoint refused the key (401)." });
  render(
    <AddAccount
      harnesses={MOCK_HARNESSES}
      kindsOf={(h) => MOCK_KINDS[h]}
      onCommand={vi.fn()}
      onClose={vi.fn()}
      testDraft={testDraft}
    />,
  );
  const d = screen.getByRole("dialog");
  fireEvent.change(within(d).getByRole("textbox", { name: "Label" }), {
    target: { value: "Work" },
  });
  const key = within(d).getByLabelText("API key", { selector: "input" });
  fireEvent.change(key, { target: { value: "sk-1" } });
  fireEvent.click(within(d).getByRole("button", { name: "Test connection" }));
  expect(testDraft).toHaveBeenCalledWith({ harness: "claude", kind: "apiKey", secret: "sk-1" });
  const ok = await within(d).findByRole("status");
  expect(ok.textContent).toContain("Connected · 2 models · 340 ms");
  expect(ok.textContent).toContain("claude-haiku-5-5");
  expect(check(document.body)).toEqual([]);
  // A changed key clears the old result; the next test says why it failed.
  fireEvent.change(key, { target: { value: "sk-2" } });
  expect(within(d).queryByRole("status")).toBeNull();
  fireEvent.click(within(d).getByRole("button", { name: "Test connection" }));
  expect((await within(d).findByRole("status")).textContent).toContain("refused the key (401)");
});

it("offers no Sign in or Test connection it can't run", () => {
  render(
    <AddAccount
      harnesses={MOCK_HARNESSES}
      kindsOf={(h) => MOCK_KINDS[h]}
      onCommand={vi.fn()}
      onClose={vi.fn()}
    />,
  );
  const d = screen.getByRole("dialog");
  expect(within(d).queryByRole("radio", { name: "Sign in" })).toBeNull();
  expect(within(d).queryByRole("button", { name: "Test connection" })).toBeNull();
  expect(d.textContent).not.toMatch(/Not available yet/);
});
