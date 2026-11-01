import { expect, it } from "vitest";
import { stepLine, type JobState } from "./setup";

const job = (patch: Partial<JobState>): JobState => ({
  id: "job_1",
  title: "Set up 3 things",
  status: "running",
  steps: [
    { label: "Installing Git", status: "done" },
    { label: "Installing GitHub CLI", status: "running" },
    { label: "Installing Node.js LTS", status: "pending" },
  ],
  percent: 50,
  ...patch,
});

it("says the current step in plain words, with where it is in the batch", () => {
  expect(stepLine(job({}))).toBe("Installing GitHub CLI · 2 of 3");
  expect(stepLine(job({ status: "done" }))).toBe("Set up finished");
  expect(stepLine(job({ status: "failed", error: "The download was interrupted." }))).toBe(
    "The download was interrupted.",
  );
});

it("finds a finished step's plain line for a harness", async () => {
  const { doneNote } = await import("./setup");
  const j = job({
    status: "done",
    steps: [
      { label: "Updating Codex", status: "done", note: "Codex is already up to date." },
      { label: "Installing Git", status: "done" },
    ],
  });
  expect(doneNote([j], "Codex")).toBe("Codex is already up to date.");
  expect(doneNote([j], "Git")).toBeUndefined();
});
