import { useState } from "react";
import {
  ArrowUpCircleIcon,
  CheckCircle2Icon,
  CircleAlertIcon,
  LoaderIcon,
  RefreshCwIcon,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { isRegistered, liveBackend, runCommand, useAppState } from "@/lib/appState";
import { notify } from "@/lib/notify";
import { prepareRestart } from "@/lib/restart";
import { errorText } from "@/lib/types";
import { ago, useStatuses } from "@/features/workspace/status";

/** `/update` (src-tauri/src/updater.rs). */
export type UpdateState = {
  current: string;
  phase: "idle" | "checking" | "downloading" | "installing";
  checkedAt?: string;
  available?: { version: string; notes?: string; date?: string };
  percent?: number;
  error?: string;
};

export const useUpdate = () => useAppState((t) => t.update as UpdateState | undefined);

/**
 * Restart to update: saves the sessions and every terminal's screen first, then downloads the new
 * version and restarts into it. The panes come back where they were and the agents pick up their
 * conversations.
 */
export async function restartToUpdate() {
  try {
    await prepareRestart();
    await runCommand("app.update.install", {});
  } catch (e) {
    notify.error("Couldn't update Gond Code", { description: errorText(e) });
  }
}

/** Settings → Updates: this version, Check for updates, and Restart to update when there's one. */
export function UpdateSettings({ version }: { version?: string }) {
  const update = useUpdate();
  const statuses = useStatuses();
  const [asked, setAsked] = useState(false);
  const live = liveBackend() && isRegistered("app.update.check");
  const current = update?.current ?? version;
  const phase = update?.phase ?? "idle";
  const busy = phase !== "idle";
  const working = Object.values(statuses).filter((s) => s.status === "working").length;
  const available = update?.available;

  return (
    <section aria-label="Updates" className="flex max-w-xl flex-col gap-5">
      <header>
        <h2 className="text-lg font-semibold">Updates</h2>
        <p className="text-sm text-muted-foreground">
          {current ? `Gond Code ${current}` : "Gond Code"}
        </p>
      </header>

      <div className="flex flex-col gap-4 rounded-2xl border border-edge bg-raised p-4">
        {available ? (
          <>
            <div className="flex items-start gap-3">
              <ArrowUpCircleIcon className="mt-0.5 size-5 shrink-0 text-tint-ink" />
              <div className="flex min-w-0 flex-col gap-1">
                <p className="font-medium">Gond Code {available.version} is ready</p>
                <p className="text-xs text-muted-foreground">
                  Your sessions, panes and agents come back where you left them.
                </p>
              </div>
            </div>
            {available.notes && (
              <p
                data-selectable
                className="max-h-56 overflow-y-auto rounded-xl bg-background/60 p-3 text-xs leading-relaxed whitespace-pre-wrap text-muted-foreground"
              >
                {plainNotes(available.notes)}
              </p>
            )}
            {phase === "downloading" || phase === "installing" ? (
              <div className="flex items-center gap-3" role="status">
                <span className="block h-1.5 flex-1 overflow-hidden rounded-full bg-hover">
                  <span
                    className="block h-full rounded-full bg-tint transition-[width]"
                    style={{ width: `${update?.percent ?? 0}%` }}
                  />
                </span>
                <span className="text-xs text-muted-foreground tabular-nums">
                  {phase === "installing" ? "Restarting…" : `Downloading… ${update?.percent ?? 0}%`}
                </span>
              </div>
            ) : (
              <div className="flex flex-wrap items-center gap-3">
                <Button
                  className="rounded-full"
                  data-command="app.update.install"
                  disabled={!live || asked}
                  onClick={() => {
                    setAsked(true);
                    void restartToUpdate().finally(() => setAsked(false));
                  }}
                >
                  Restart to update
                </Button>
                {working > 0 && (
                  <p className="text-xs text-muted-foreground">
                    {working === 1 ? "An agent is" : `${working} agents are`} working: a reply in
                    progress stops, then the conversation resumes.
                  </p>
                )}
              </div>
            )}
          </>
        ) : (
          <div className="flex items-center gap-3">
            {phase === "checking" ? (
              <LoaderIcon className="size-5 shrink-0 text-muted-foreground motion-safe:animate-spin" />
            ) : update?.error ? (
              <CircleAlertIcon className="size-5 shrink-0 text-tone-bad" />
            ) : (
              <CheckCircle2Icon className="size-5 shrink-0 text-tone-ok" />
            )}
            <div className="flex min-w-0 flex-col gap-0.5">
              <p className="font-medium">
                {phase === "checking"
                  ? "Checking for updates…"
                  : update?.error
                    ? "Couldn't check for updates"
                    : "You're up to date"}
              </p>
              {phase !== "checking" && (update?.error || update?.checkedAt) && (
                <p
                  role={update?.error ? "alert" : undefined}
                  className="text-xs text-muted-foreground"
                >
                  {update?.error ?? `Checked ${checkedAgo(update!.checkedAt!)}`}
                </p>
              )}
            </div>
          </div>
        )}
        {available && update?.error && (
          <p role="alert" className="text-xs text-tone-bad">
            {update.error}
          </p>
        )}
      </div>

      <div className="flex flex-wrap items-center gap-3">
        <Button
          variant="outline"
          className="rounded-full"
          data-command="app.update.check"
          disabled={!live || busy}
          onClick={() =>
            void runCommand("app.update.check", {}).catch((e) =>
              notify.error("Couldn't check for updates", { description: errorText(e) }),
            )
          }
        >
          <RefreshCwIcon data-icon="inline-start" /> Check for updates
        </Button>
        <p className="text-xs text-muted-foreground">Also checks on its own every few hours.</p>
      </div>
    </section>
  );
}

/** Release notes are light Markdown; shown as text, without the heading and bold marks. */
function plainNotes(notes: string) {
  return notes.replace(/^#{1,6}\s+/gm, "").replace(/\*\*(.+?)\*\*/g, "$1");
}

function checkedAgo(at: string) {
  const t = Date.parse(at);
  if (Number.isNaN(t)) return "";
  const a = ago(t);
  return a === "now" ? "just now" : `${a} ago`;
}
