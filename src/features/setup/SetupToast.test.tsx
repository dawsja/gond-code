// @vitest-environment happy-dom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { check } from "../../../e2e/legibility/legibility";
import { DONE_MS, SetupToast } from "./SetupToast";
import { MOCK_JOB, MOCK_JOB_FAILED, MOCK_JOB_INSTALLING } from "./mock";

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

it("shows the percent and the step, and Hide never cancels anything", () => {
  const onHide = vi.fn();
  const onCommand = vi.fn();
  render(<SetupToast job={MOCK_JOB} onHide={onHide} onGone={onCommand} />);
  expect(check(document.body)).toEqual([]);
  expect(screen.getByRole("progressbar").getAttribute("aria-valuenow")).toBe("64");
  expect(screen.getByText("Downloading Git")).toBeTruthy();
  expect(screen.getByText("36 MB of 57 MB · 3.1 MB/s")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Hide" }));
  expect(onHide).toHaveBeenCalledTimes(1);
  expect(onCommand).not.toHaveBeenCalled();
});

it("after the download, an indeterminate bar and the time spent", () => {
  render(<SetupToast job={MOCK_JOB_INSTALLING} onHide={vi.fn()} onGone={vi.fn()} />);
  expect(check(document.body)).toEqual([]);
  expect(screen.getByText("Installing Git")).toBeTruthy();
  expect(screen.getByRole("progressbar").getAttribute("aria-valuenow")).toBeNull();
});

it("says Set up finished for 5 seconds, then goes", () => {
  vi.useFakeTimers();
  const onGone = vi.fn();
  const done = {
    ...MOCK_JOB,
    status: "done" as const,
    steps: [{ label: "Installing Git", status: "done" as const }],
  };
  render(<SetupToast job={done} onHide={() => {}} onGone={onGone} />);
  expect(screen.getByText("Set up finished")).toBeTruthy();
  expect(screen.getByText("Git · ready")).toBeTruthy();
  act(() => vi.advanceTimersByTime(DONE_MS - 1));
  expect(onGone).not.toHaveBeenCalled();
  act(() => vi.advanceTimersByTime(1));
  expect(onGone).toHaveBeenCalledTimes(1);
});

it("a failure is one plain sentence", () => {
  render(<SetupToast job={MOCK_JOB_FAILED} onHide={() => {}} onGone={() => {}} />);
  expect(
    screen.getByText("Git couldn't install because Windows needs to restart first."),
  ).toBeTruthy();
  expect(screen.queryByRole("progressbar")).toBeNull();
});

it("a done step with a plain line shows it instead of 'ready'", () => {
  render(
    <SetupToast
      job={{
        ...MOCK_JOB,
        status: "done",
        steps: [{ label: "Updating Codex", status: "done", note: "Codex is already up to date." }],
      }}
      onHide={() => {}}
      onGone={() => {}}
    />,
  );
  expect(screen.getByText("Codex is already up to date.")).toBeTruthy();
});
