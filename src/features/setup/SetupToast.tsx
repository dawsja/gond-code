import { useEffect } from "react";
import { CheckIcon, LoaderIcon, TriangleAlertIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { stepLine, type JobState } from "@/lib/setup";
import { JobProgress } from "./JobProgress";
import { Hint } from "@/components/hint";

/** How long "Set up finished" stays. */
export const DONE_MS = 5000;

type Props = {
  job: JobState;
  /** Hiding never cancels the work; the title bar's "Setting up…" chip brings it back. */
  onHide: () => void;
  /** The job finished and its 5 seconds are up. */
  onGone: () => void;
};

/**
 * The background set-up, bottom-right in the app: percent, the current step in plain
 * words, and Hide. The person keeps working the whole time.
 */
export function SetupToast({ job, onHide, onGone }: Props) {
  const done = job.status === "done";
  const failed = job.status === "failed" || job.status === "cancelled";
  useEffect(() => {
    if (!done) return;
    const t = setTimeout(onGone, DONE_MS);
    return () => clearTimeout(t);
  }, [done, onGone]);

  return (
    <section
      role="status"
      aria-label="Setting up"
      data-entity="install-job"
      data-id={job.id}
      className="fixed right-4 bottom-4 flex w-90 flex-col gap-3 rounded-2xl border border-edge bg-card p-4 shadow-lg min-[1920px]:right-6 min-[1920px]:bottom-6"
    >
      <header className="flex items-center gap-3">
        {done ? (
          <CheckIcon className="size-4 shrink-0 text-tone-ok" />
        ) : failed ? (
          <TriangleAlertIcon className="size-4 shrink-0 text-tone-bad" />
        ) : (
          <LoaderIcon aria-hidden className="size-4 shrink-0 text-tint motion-safe:animate-spin" />
        )}
        <Hint text={done ? "Set up finished" : job.title}>
          <p className="min-w-0 flex-1 truncate font-medium">
            {done ? "Set up finished" : job.title}
          </p>
        </Hint>
        {!done && (
          <Button
            variant="ghost"
            size="sm"
            className="shrink-0"
            data-local="setup-toast-hide"
            onClick={onHide}
          >
            Hide
          </Button>
        )}
      </header>
      {done ? (
        <ul className="flex flex-col gap-1 text-sm text-muted-foreground">
          {job.steps.map((s) => (
            <Hint text={s.note ?? s.label}>
              <li key={s.label} className="truncate">
                {/* A plain line from the installer wins over "ready". */}
                {s.note ?? `${s.label.replace(/^(Installing|Updating) /, "")} · ready`}
              </li>
            </Hint>
          ))}
        </ul>
      ) : failed ? (
        <p className="text-sm text-tone-bad">{job.error ?? "The set-up stopped."}</p>
      ) : (
        <>
          {job.steps.length > 1 && (
            <Hint text={stepLine(job)}>
              <p className="truncate text-xs text-muted-foreground">{stepLine(job)}</p>
            </Hint>
          )}
          <JobProgress job={job} />
        </>
      )}
    </section>
  );
}
