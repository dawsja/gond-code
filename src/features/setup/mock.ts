// Set-up jobs (Settings → GitHub installs Git and gh) for the mock build's toast (?toast) and the
// tests: one running, one installing, one failed.
import type { JobState } from "@/lib/setup";

export const MOCK_JOB: JobState = {
  id: "job_setup_1",
  title: "Set up Git",
  status: "running",
  steps: [{ label: "Installing Git", status: "running" }],
  // real bytes while downloading.
  stage: "downloading",
  stageStartedAt: new Date(Date.now() - 6_000).toISOString(),
  bytes: { done: 36 * 1024 * 1024, total: 57 * 1024 * 1024 },
  speed: 3.1 * 1024 * 1024,
  percent: 64,
};

/** The same job after the download: no percent, so an indeterminate bar and the time spent. */
export const MOCK_JOB_INSTALLING: JobState = {
  ...MOCK_JOB,
  stage: "installing",
  stageStartedAt: new Date(Date.now() - 23_000).toISOString(),
  bytes: undefined,
  speed: undefined,
  percent: undefined,
};

export const MOCK_JOB_FAILED: JobState = {
  ...MOCK_JOB,
  status: "failed",
  steps: [{ label: "Installing Git", status: "failed" }],
  percent: undefined,
  error: "Git couldn't install because Windows needs to restart first.",
  details:
    "winget install --id Git.Git --exact --silent --scope user\nFound Git [Git.Git] Version 2.47.1\nDownloading https://github.com/git-for-windows/git/releases/download/v2.47.1.windows.1/Git-2.47.1-64-bit.exe\nSuccessfully verified installer hash\nStarting package install...\nInstaller failed with exit code: 3010 (a restart is required)",
};
