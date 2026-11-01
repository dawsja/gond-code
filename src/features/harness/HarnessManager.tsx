import { InstallControl } from "./InstallControl";
import { UninstallButton, UninstallDialog } from "./UninstallDialog";
import type { useHarnessInstalls } from "./useHarnessInstalls";
import { useEffect, useState } from "react";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { isRegistered } from "@/lib/appState";
import { doneNote, NOTE_MS, type JobState } from "@/lib/setup";
import {
  ArrowUpCircleIcon,
  DownloadIcon,
  MoreHorizontalIcon,
  RefreshCwIcon,
  Trash2Icon,
} from "lucide-react";
import { ToneBadge } from "@/components/gond/badges";
import { openExternal } from "@/lib/ipc";
import { ProviderIcon } from "@/components/brand/provider-icon";
import { Button } from "@/components/ui/button";
import type { RunCommand } from "@/features/deck/model";
import { statusOf, type Harness, type HarnessStatus } from "@/lib/harnesses";
import { Hint } from "@/components/hint";

const STATUS: Record<HarnessStatus, { label: string; tone: "ok" | "info" | "neutral" | "bad" }> = {
  "up-to-date": { label: "Up to date", tone: "ok" },
  update: { label: "Update available", tone: "info" },
  missing: { label: "Not installed", tone: "neutral" },
  broken: { label: "Broken", tone: "bad" },
  self: { label: "Updates itself", tone: "neutral" },
};

/**
 * Settings → Harnesses: the agent programs Gond Code runs. Updates are found by themselves and
 * installed by you, after a plain "Update X to Y?" (what changes and a link to what's new).
 */
export function HarnessManager({
  harnesses,
  running = {},
  onCommand,
  jobs = [],
  installs,
  checkUpdates,
}: {
  harnesses: Harness[];
  /** Panes open on each agent (by harness id): it can't be uninstalled under them. */
  running?: Record<string, number>;
  onCommand: RunCommand;
  /** /installs: a finished update's plain line shows under its harness. */
  jobs?: JobState[];
  /** Live installs, updates and uninstalls: the same job path as first run. */
  installs?: Pick<
    ReturnType<typeof useHarnessInstalls>,
    "install" | "update" | "uninstall" | "state"
  >;
  /** Looks for newer versions now (they're also looked for on their own, every few hours). */
  checkUpdates?: () => void;
}) {
  const [removing, setRemoving] = useState<string>();
  const [asking, setAsking] = useState<string>();
  const check = () => (checkUpdates ? checkUpdates() : onCommand("harness.check", {}));
  const ask = harnesses.find((h) => h.id === asking);
  // Live: the shared job path, so the card shows progress, then the version, or the error with
  // Retry. Demo and tests without one: the same commands with their real inputs.
  const install = (id: string) =>
    installs ? installs.install(id) : onCommand("harness.install", { harnessId: id });
  const update = (id: string) =>
    installs ? installs.update(id) : onCommand("harness.update", { harnessId: id });
  const busy = (id: string) => {
    const st = installs?.state(id);
    return !!st && (st.starting || (!!st.job && st.job.status === "running"));
  };

  return (
    <div className="flex gap-6 @max-[1200px]:flex-col">
      <section aria-label="Harnesses" className="@container flex min-w-0 flex-1 flex-col gap-4">
        <header className="flex flex-wrap items-start gap-3">
          <div className="min-w-0 flex-1">
            <h2 className="text-lg font-semibold">Harnesses</h2>
            <p className="max-w-[72ch] text-sm text-muted-foreground">
              The agent programs you run in your terminals. Install and update them here.
            </p>
          </div>
          <Button variant="outline" data-command="harness.check" onClick={check}>
            <RefreshCwIcon /> Check all
          </Button>
        </header>
        <ul className="grid gap-3 @min-[46rem]:grid-cols-2">
          {harnesses.map((h) => {
            const status = statusOf(h);
            const n = h.installed ? (running[h.id] ?? 0) : 0;
            return (
              <HarnessCard
                key={h.id}
                harness={h}
                status={status}
                running={n}
                asking={h.id === asking}
                jobs={jobs}
                installs={installs}
                busy={busy(h.id)}
                onInstall={() => install(h.id)}
                onUpdate={() => update(h.id)}
                onAskUpdate={() => setAsking(h.id)}
                onCheck={check}
                onUninstall={() => setRemoving(h.id)}
              />
            );
          })}
        </ul>
        {/* Updates are found on their own and installed only from a card's Update button
: no automatic installs. */}
      </section>

      {removing && (
        <UninstallDialog
          harness={harnesses.find((h) => h.id === removing)!}
          onClose={() => setRemoving(undefined)}
          onUninstall={(removeEverything) => {
            const id = removing;
            setRemoving(undefined);
            if (installs) installs.uninstall(id, removeEverything);
            else onCommand("harness.uninstall", { harnessId: id, keepData: !removeEverything });
          }}
        />
      )}
      {ask && (
        <UpdateCard
          harness={ask}
          onAllow={() => {
            update(ask.id);
            setAsking(undefined);
          }}
          onDeny={() => setAsking(undefined)}
        />
      )}
    </div>
  );
}

/**
 * One harness in Settings: who it is, version → latest, source and the panes running it, then
 * its actions in one footer row pinned to the bottom so the grid's rows line up: the primary
 * action (Install, Update or Check) on the left, a muted Uninstall on the right (in a ⋯ menu on a
 * narrow card).
 */
/**
 * A finished job's line for this agent ("Pi is ready.", "Pi is uninstalled."), while it's fresh:
 * it clears `NOTE_MS` after the job ends, and isn't there when you come back later.
 */
function useFreshNote(jobs: JobState[], name: string): string | undefined {
  const [now, setNow] = useState(() => Date.now());
  const note = doneNote(jobs, name, now);
  // The latest end among the jobs: wake once its line is due to go.
  const last = Math.max(0, ...jobs.map((j) => (j.endedAt ? Date.parse(j.endedAt) : 0)));
  useEffect(() => {
    const left = last + NOTE_MS - Date.now();
    if (left <= 0) return;
    const t = window.setTimeout(() => setNow(Date.now()), left + 50);
    return () => window.clearTimeout(t);
  }, [last]);
  // A job that ends after `now` was read counts as fresh until that timer reads it again.
  return note;
}

function HarnessCard({
  harness: h,
  status,
  running: n,
  asking,
  jobs,
  installs,
  busy,
  onInstall,
  onUpdate,
  onAskUpdate,
  onCheck,
  onUninstall,
}: {
  harness: Harness;
  status: HarnessStatus;
  running: number;
  asking: boolean;
  jobs: JobState[];
  installs?: Pick<ReturnType<typeof useHarnessInstalls>, "uninstall" | "state">;
  busy: boolean;
  onInstall: () => void;
  onUpdate: () => void;
  onAskUpdate: () => void;
  onCheck: () => void;
  onUninstall: () => void;
}) {
  const state = installs?.state(h.id);
  const note = useFreshNote(jobs, h.label);
  const canUninstall = h.installed && status !== "self";
  const uninstallWhy = n ? `Close its ${n === 1 ? "pane" : `${n} panes`} first` : undefined;
  const uninstallOff =
    !isRegistered("harness.uninstallPlan") || !!uninstallWhy || state?.kind === "uninstall";
  const primary =
    (status === "update" || status === "missing") && busy ? null : status === "update" ||
      status === "missing" ? (
      <Button
        size="sm"
        variant={status === "update" ? "default" : "outline"}
        className="rounded-full"
        data-command={status === "update" ? "harness.update" : "harness.install"}
        disabled={!isRegistered(status === "update" ? "harness.update" : "harness.install")}
        onClick={status === "update" ? onAskUpdate : onInstall}
      >
        {status === "update" ? <ArrowUpCircleIcon /> : <DownloadIcon />}
        {status === "update" ? "Update" : "Install"}
      </Button>
    ) : status !== "self" ? (
      <Button
        size="sm"
        variant="ghost"
        className="rounded-full"
        data-command="harness.check"
        onClick={onCheck}
      >
        <RefreshCwIcon /> Check
      </Button>
    ) : null;

  return (
    <li
      data-entity="harness"
      data-id={h.id}
      data-layout="harness-card"
      className="@container flex min-w-0 flex-col gap-3 rounded-3xl border border-edge bg-card p-4 data-[asking]:border-tint"
      data-asking={asking ? "" : undefined}
    >
      <div className="flex items-start gap-3">
        <span className="grid size-10 shrink-0 place-items-center rounded-xl border border-edge bg-raised">
          <ProviderIcon id={h.brand} size={20} />
        </span>
        <div className="min-w-0 flex-1">
          <Hint text={h.label}>
            <p className="truncate font-semibold">{h.label}</p>
          </Hint>
          {h.source && (
            <Hint text={h.source.name}>
              <p className="truncate text-xs text-muted-foreground">
                {h.source.kind} · {h.source.name}
              </p>
            </Hint>
          )}
        </div>
        <ToneBadge tone={STATUS[status].tone} className="shrink-0">
          {STATUS[status].label}
        </ToneBadge>
      </div>
      <p className="flex flex-wrap items-baseline gap-x-2 text-sm">
        <span className="font-mono tabular-nums">
          {h.installed ? (
            h.version
          ) : (
            <span className="font-sans text-muted-foreground">Not on this PC</span>
          )}
          {status === "update" && <span className="text-tint-ink"> → {h.latest}</span>}
        </span>
        <span className="text-xs text-muted-foreground">
          {n ? `${n} pane${n > 1 ? "s" : ""} open` : "Idle"}
        </span>
      </p>
      {state && (
        <InstallControl
          id={h.id}
          label={h.label}
          state={state}
          onInstall={() =>
            state.kind === "uninstall"
              ? installs!.uninstall(h.id)
              : state.kind === "update"
                ? onUpdate()
                : onInstall()
          }
        />
      )}
      {!state && note && (
        <Hint text={note}>
          <p role="status" className="truncate text-xs text-tone-ok">
            {note}
          </p>
        </Hint>
      )}
      <footer className="mt-auto flex min-h-8 items-center gap-1">
        {primary}
        <span className="flex-1" />
        {/* Wide card: the quiet actions in a row. */}
        <span className="flex items-center gap-1 @max-[26rem]:hidden">
          {canUninstall && (
            <UninstallButton
              harness={h}
              disabled={state?.kind === "uninstall"}
              why={uninstallWhy}
              onClick={onUninstall}
            />
          )}
        </span>
        {/* Narrow card: it folds into ⋯. */}
        {canUninstall && (
          <span className="@min-[26rem]:hidden">
            <DropdownMenu>
              <DropdownMenuTrigger
                aria-label={`More for ${h.label}`}
                data-local="harness-menu"
                data-arg={h.id}
                className="grid size-8 place-items-center rounded-full text-muted-foreground outline-none hover:bg-hover focus-visible:ring-2 focus-visible:ring-tint"
              >
                <MoreHorizontalIcon className="size-4" />
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                {/* A grid, so the item spans the menu (and its highlight). */}
                <Hint text={uninstallWhy} wrapClassName="grid">
                  <DropdownMenuItem
                    data-command="harness.uninstall"
                    data-arg={h.id}
                    className="text-muted-foreground"
                    disabled={uninstallOff}
                    onSelect={onUninstall}
                  >
                    <Trash2Icon /> Uninstall
                  </DropdownMenuItem>
                </Hint>
              </DropdownMenuContent>
            </DropdownMenu>
          </span>
        )}
      </footer>
    </li>
  );
}

/**
 * Install or update an agent, confirmed (a plain question, not an approval card): what changes, its release notes if it has any, then Update now or Not yet.
 */
function UpdateCard({
  harness: h,
  onAllow,
  onDeny,
}: {
  harness: Harness;
  onAllow: () => void;
  onDeny: () => void;
}) {
  const verb = h.installed ? "Update" : "Install";
  return (
    <article
      data-entity="harness-update"
      data-id={h.id}
      aria-label={`${verb} ${h.label}?`}
      className="flex w-100 shrink-0 flex-col gap-4 self-start rounded-3xl border border-edge bg-card p-5 @max-[1200px]:order-first @max-[1200px]:w-full"
    >
      <header className="flex items-start gap-3">
        <span className="grid size-10 shrink-0 place-items-center rounded-full border border-edge bg-raised">
          <ProviderIcon id={h.brand} size={20} />
        </span>
        <div className="min-w-0 flex-1">
          <p className="text-[15px] font-semibold">
            {verb} {h.label}
            {h.latest && ` to ${h.latest}`}?
          </p>
          {h.installed && <p className="text-xs text-muted-foreground">You have {h.version}</p>}
        </div>
      </header>
      {h.changelog && (
        <ul className="flex list-disc flex-col gap-1 rounded-2xl border border-edge p-4 pl-8 text-sm">
          {h.changelog.slice(0, 3).map((line) => (
            <li key={line}>{line}</li>
          ))}
        </ul>
      )}
      <p className="text-sm text-muted-foreground">
        A chat that&apos;s working keeps the version it started with; its next message uses the new
        one.
        {h.notesUrl && (
          <>
            {" "}
            <button
              type="button"
              data-local="harness-notes"
              onClick={() => void openExternal(h.notesUrl!)}
              className="text-tint-ink underline-offset-2 outline-none hover:underline focus-visible:underline"
            >
              What&apos;s new
            </button>
          </>
        )}
      </p>
      <div className="flex flex-col gap-2">
        <Button
          size="lg"
          data-command={h.installed ? "harness.update" : "harness.install"}
          onClick={onAllow}
        >
          {verb} now
        </Button>
        <Button size="lg" variant="outline" data-local="not-yet" onClick={onDeny}>
          Not yet
        </Button>
      </div>
    </article>
  );
}
