import { Elapsed } from "./elapsed";
import { useTicking } from "./useTicking";
import { stageLine, type JobState } from "@/lib/setup";
import { cn } from "@/lib/utils";
import { Hint } from "@/components/hint";

/**
 * An install job's running step, honestly (Anvil): the stage in words, and while
 * downloading a real bar with "12 MB of 55 MB · 3.1 MB/s"; for installing, setting up and
 * checking, an indeterminate bar and the time spent, never a frozen percent.
 */
export function JobProgress({ job, className }: { job: JobState; className?: string }) {
  const line = stageLine(job);
  const seconds = useTicking(line.determinate ? undefined : job.stageStartedAt);
  const percent = Math.max(0, Math.min(100, job.percent ?? 0));
  return (
    <div
      data-entity="install-progress"
      data-id={job.id}
      data-stage={job.stage}
      className={cn("flex min-w-0 flex-col gap-1.5", className)}
    >
      <div className="flex min-w-0 items-baseline justify-between gap-3 text-sm">
        <Hint text={line.title}>
          <span className="min-w-0 truncate font-medium">{line.title}</span>
        </Hint>
        <span className="shrink-0 text-xs text-muted-foreground tabular-nums">
          {line.determinate
            ? `${Math.round(percent)}%`
            : job.stageStartedAt && <Elapsed seconds={seconds} />}
        </span>
      </div>
      <div
        role="progressbar"
        aria-label={line.title}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={line.determinate ? Math.round(percent) : undefined}
        aria-valuetext={
          line.determinate ? (line.detail ?? `${Math.round(percent)}%`) : `${line.title}, working`
        }
        className="relative h-1 overflow-hidden rounded-full bg-raised"
      >
        {line.determinate ? (
          <div
            className="h-full rounded-full bg-tint transition-[width]"
            style={{ width: `${percent}%` }}
          />
        ) : (
          <div className="absolute inset-y-0 w-1/3 rounded-full bg-tint motion-safe:animate-[indeterminate_1.4s_ease-in-out_-0.5s_infinite] motion-reduce:w-full motion-reduce:opacity-40" />
        )}
      </div>
      {line.detail && (
        <Hint text={line.detail}>
          <span className="truncate text-xs text-muted-foreground tabular-nums">{line.detail}</span>
        </Hint>
      )}
    </div>
  );
}
