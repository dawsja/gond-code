import { DownloadIcon, RotateCwIcon } from "lucide-react";
import { Hint } from "@/components/hint";
import { Button } from "@/components/ui/button";
import { runCommand } from "@/lib/appState";
import { JobProgress } from "@/features/setup/JobProgress";
import type { InstallState } from "./useHarnessInstalls";

/** A harness card's install: the button, then the job's progress, then a plain error with
 *  Retry. Installed comes from re-detecting the agents, so the card turns "Found" on its own. */
export function InstallControl({
  id,
  label,
  state,
  onInstall,
}: {
  id: string;
  label: string;
  state?: InstallState;
  onInstall: () => void;
}) {
  const job = state?.job;
  const command =
    state?.kind === "uninstall"
      ? "harness.uninstall"
      : state?.kind === "update"
        ? "harness.update"
        : "harness.install";
  if (state?.error)
    return (
      <div className="flex flex-col gap-2" role="alert">
        <Hint text={job?.details || undefined}>
          <p className="text-xs text-tone-bad">{state.error}</p>
        </Hint>
        <Button
          variant="outline"
          size="sm"
          className="w-fit rounded-full"
          data-command={command}
          data-arg={id}
          onClick={onInstall}
        >
          <RotateCwIcon data-icon="inline-start" /> Retry
        </Button>
      </div>
    );
  if (state?.starting || (job && job.status !== "done"))
    return job ? (
      <div className="flex flex-col gap-2">
        <JobProgress job={job} />
        {job.status === "running" && (
          // Stops the installer (its whole process tree); the job ends as failed with a plain line.
          <Button
            variant="ghost"
            size="sm"
            className="w-fit rounded-full text-muted-foreground"
            data-command="install.cancel"
            data-arg={id}
            onClick={() =>
              void runCommand("install.cancel", { jobId: job.id }).catch(() => undefined)
            }
          >
            Cancel
          </Button>
        )}
      </div>
    ) : (
      <p className="text-sm text-muted-foreground" data-id={`install-${id}`}>
        {state?.kind === "uninstall" ? "Uninstalling" : "Starting"} {label}…
      </p>
    );
  if (job?.status === "done")
    return (
      <p role="status" className="text-xs text-tone-ok">
        {job.steps.find((s) => s.note)?.note ??
          `${label} ${state?.kind === "uninstall" ? "uninstalled" : state?.kind === "update" ? "updated" : "installed"}. Checking…`}
      </p>
    );
  return (
    <Button
      variant="outline"
      size="sm"
      className="w-fit rounded-full"
      data-command={command}
      data-arg={id}
      onClick={onInstall}
    >
      <DownloadIcon data-icon="inline-start" /> Install
    </Button>
  );
}
