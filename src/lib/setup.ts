// Background setup: prerequisites, the install job that sets them up,
// and the Git identity. Shapes of prereq.list / prereq.install and /installs/{jobId}
//  and Anvil's git.identity.

export type Prereq = { id: string; name: string; installed: boolean; version?: string };

export type JobStatus = "pending" | "running" | "done" | "failed" | "cancelled";

/** `/installs/{jobId}`: one background job as the card and the toast see it. */
export type JobState = {
  id: string;
  title: string;
  status: JobStatus;
  steps: {
    label: string;
    status: JobStatus;
    /** A done step's plain line: "Codex is already up to date.", "…restart to finish". */
    note?: string;
  }[];
  /** what the running step is doing now, and since when. Only "downloading" has a
   *  real percent; the other stages show an indeterminate bar with the time spent. */
  stage?: "downloading" | "installing" | "settingUp" | "checking";
  stageStartedAt?: string;
  /** Bytes downloaded so far, and the size when the server says it. */
  bytes?: { done: number; total?: number };
  /** Download speed, bytes per second. */
  speed?: number;
  /** The running step's progress: only while downloading. */
  percent?: number;
  /** One plain sentence; `details` has the captured output for "Show details". */
  error?: string;
  details?: string;
  /** winget had no per-user installer: offer "Install for all users" (one Windows prompt). */
  machineScopeAvailable?: boolean;
};

export type Identity = { name?: string; email?: string };
export type IdentityStatus = { current: Identity; needed: boolean; suggested?: Identity };

/** The one-line step in plain words: "Installing Git · 2 of 3". */
export function stepLine(job: JobState): string {
  const i = job.steps.findIndex((s) => s.status === "running" || s.status === "pending");
  if (job.status === "done") return "Set up finished";
  if (job.status === "failed") return job.error ?? "Set up stopped";
  if (i < 0) return "Checking…";
  const n = job.steps.length;
  return n > 1 ? `${job.steps[i].label} · ${i + 1} of ${n}` : job.steps[i].label;
}

/** The note on the last finished step that names this harness or tool ("Codex is already up to
 *  date."), from any job: shown under it in the harness manager and on its set-up card. */
export function doneNote(jobs: JobState[], name: string): string | undefined {
  for (const j of [...jobs].reverse())
    for (const s of j.steps)
      if (s.status === "done" && s.note && (s.label.endsWith(name) || s.note.startsWith(name)))
        return s.note;
  return undefined;
}

const STAGE: Record<NonNullable<JobState["stage"]>, string> = {
  downloading: "Downloading",
  installing: "Installing",
  settingUp: "Setting up",
  checking: "Checking",
};

/** "12 MB", "1.4 GB": sizes the way Windows shows them. */
export function size(bytes: number): string {
  const units = ["B", "KB", "MB", "GB"];
  let v = bytes;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v >= 10 || i === 0 ? Math.round(v) : v.toFixed(1)} ${units[i]}`;
}

/** The running step in words: "Downloading Git", plus "12 MB of 55 MB · 3.1 MB/s" while
 *  downloading. Never a made-up number: without a stage, the step's own label. */
export function stageLine(job: JobState): { title: string; detail?: string; determinate: boolean } {
  const step = job.steps.find((s) => s.status === "running");
  const name =
    step?.label.replace(
      /^(Installing|Install|Updating|Update|Downloading|Download|Setting up|Set up) /,
      "",
    ) ?? job.title;
  if (!job.stage)
    return { title: step?.label ?? job.title, determinate: job.percent !== undefined };
  const title = `${STAGE[job.stage]} ${name}`;
  if (job.stage !== "downloading") return { title, determinate: false };
  const parts = [
    job.bytes &&
      (job.bytes.total
        ? `${size(job.bytes.done)} of ${size(job.bytes.total)}`
        : size(job.bytes.done)),
    job.speed ? `${size(job.speed)}/s` : undefined,
  ].filter(Boolean);
  return { title, detail: parts.join(" · ") || undefined, determinate: job.percent !== undefined };
}
