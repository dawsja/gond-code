import { expect, it } from "vitest";
import { size, stageLine, type JobState } from "./setup";

const job: JobState = {
  id: "j",
  title: "Set up Git",
  status: "running",
  steps: [{ label: "Installing Git", status: "running" }],
};

it("sizes read in B, KB, MB and GB", () => {
  expect(size(512)).toBe("512 B");
  expect(size(12 * 1024 * 1024)).toBe("12 MB");
  expect(size(3.1 * 1024 * 1024)).toBe("3.1 MB");
});

it("setting up reads Setting up Git", () => {
  expect(stageLine({ ...job, stage: "settingUp" }).title).toBe("Setting up Git");
});

it("a job without a stage keeps its step label", () => {
  expect(stageLine(job)).toEqual({ title: "Installing Git", determinate: false });
});

it('a step labelled "Install Git" reads "Installing Git", never "Installing Install Git"', () => {
  expect(
    stageLine({ ...job, steps: [{ label: "Install Git", status: "running" }], stage: "installing" })
      .title,
  ).toBe("Installing Git");
});
