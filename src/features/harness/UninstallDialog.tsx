import { useEffect, useState } from "react";
import { Trash2Icon } from "lucide-react";
import { ProviderIcon } from "@/components/brand/provider-icon";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogTitle,
} from "@/components/ui/dialog";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import { isRegistered, runCommand } from "@/lib/appState";
import type { Harness } from "@/lib/harnesses";
import { errorText } from "@/lib/types";
import { Hint } from "@/components/hint";

/** `harness.uninstallPlan`'s answer (AGENT-API): exactly what `harness.uninstall` removes. */
export type UninstallPlan = {
  harnessId: string;
  methods: string[];
  deletes: string[];
  pathEntries: string[];
  keepData: boolean;
  running: number;
};

type Load = (harnessId: string, removeEverything: boolean) => Promise<UninstallPlan>;

const loadPlan: Load = (harnessId, removeEverything) =>
  runCommand<UninstallPlan>("harness.uninstallPlan", { harnessId, keepData: !removeEverything });

const CHOICES = [
  {
    id: "keep",
    title: "Keep my settings and sign-ins",
    body: "Removes the program. Install it again later and it picks up where you left off.",
  },
  {
    id: "all",
    title: "Remove everything",
    body: "Also deletes its settings and its sign-ins.",
  },
] as const;

/**
 * Uninstall's ask: what will go, exactly as `harness.uninstallPlan` lists it (every file and
 * folder, every PATH entry), with Keep my settings and sign-ins (the default) or Remove everything.
 * The plan is fetched again when the choice flips. It refuses while that harness's agents run.
 */
export function UninstallDialog({
  harness,
  onUninstall,
  onClose,
  load = loadPlan,
}: {
  harness: Pick<Harness, "id" | "label" | "brand">;
  onUninstall: (removeEverything: boolean) => void;
  onClose: () => void;
  /** For tests and the tour. */
  load?: Load;
}) {
  const [choice, setChoice] = useState<"keep" | "all">("keep");
  const all = choice === "all";
  // Each answer remembers which choice it's for, so a flip reads "Checking…" until its own plan.
  const [got, setGot] = useState<{ all: boolean; plan?: UninstallPlan; error?: string }>();
  useEffect(() => {
    let live = true;
    load(harness.id, all).then(
      (plan) => live && setGot({ all, plan }),
      (e) => live && setGot({ all, error: errorText(e) }),
    );
    return () => {
      live = false;
    };
  }, [harness.id, all, load]);
  const plan = got?.all === all ? got.plan : undefined;
  const error = got?.all === all ? got.error : undefined;

  const running = plan?.running ?? 0;
  const why = running
    ? `Stop its ${running === 1 ? "running agent" : `${running} running agents`} first`
    : !plan
      ? "Checking what it would remove…"
      : undefined;

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent
        data-entity="harness-uninstall"
        data-id={harness.id}
        className="flex max-h-[calc(100vh-3rem)] max-w-[min(36rem,calc(100vw-3rem))] flex-col gap-4 rounded-3xl p-4 sm:max-w-[min(36rem,calc(100vw-3rem))]"
      >
        <DialogTitle className="flex items-center gap-3 text-lg">
          <ProviderIcon id={harness.brand} size={22} />
          <span className="min-w-0 truncate">Uninstall {harness.label}?</span>
        </DialogTitle>
        <DialogDescription className="sr-only">
          Choose what to keep, check what will be removed, then uninstall.
        </DialogDescription>
        <RadioGroup
          value={choice}
          onValueChange={(v) => setChoice(v as typeof choice)}
          aria-label="What to remove"
          data-local="uninstall-choice"
          className="flex flex-col gap-2"
        >
          {CHOICES.map((c) => (
            <label
              key={c.id}
              htmlFor={`uninstall-${c.id}`}
              className="flex cursor-pointer items-start gap-3 rounded-lg border border-edge p-3 has-data-[state=checked]:border-tint"
            >
              <RadioGroupItem
                value={c.id}
                id={`uninstall-${c.id}`}
                data-local="uninstall-choice"
                aria-labelledby={`uninstall-${c.id}-title`}
                aria-describedby={`uninstall-${c.id}-body`}
                className="mt-0.5"
              />
              <span className="flex min-w-0 flex-col gap-0.5">
                <span id={`uninstall-${c.id}-title`} className="font-medium">
                  {c.title}
                </span>
                <span id={`uninstall-${c.id}-body`} className="text-sm text-muted-foreground">
                  {c.body}
                </span>
              </span>
            </label>
          ))}
        </RadioGroup>

        <section aria-label="What will be removed" className="flex min-h-0 flex-col gap-2">
          {error ? (
            <p role="alert" className="text-sm text-tone-bad">
              {error}
            </p>
          ) : !plan ? (
            <p role="status" className="text-sm text-muted-foreground">
              Checking what it would remove…
            </p>
          ) : (
            <div className="flex min-h-0 flex-col gap-3 overflow-y-auto rounded-lg border border-edge bg-raised/40 p-3">
              <Paths title="Deletes" paths={plan.deletes} empty="No files" />
              <Paths title="Takes off your PATH" paths={plan.pathEntries} empty="No PATH entries" />
            </div>
          )}
          {running > 0 && (
            <p role="alert" className="text-sm text-tone-warn">
              {why}
            </p>
          )}
        </section>

        <DialogFooter className="gap-2">
          <Button
            variant="ghost"
            className="rounded-full"
            data-local="uninstall-cancel"
            onClick={onClose}
          >
            Cancel
          </Button>
          <Hint text={why}>
            <Button
              variant="destructive"
              className="rounded-full"
              data-command="harness.uninstall"
              data-arg={harness.id}
              disabled={!!why}
              aria-description={why}
              onClick={() => onUninstall(all)}
            >
              <Trash2Icon data-icon="inline-start" /> {all ? "Remove everything" : "Uninstall"}
            </Button>
          </Hint>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function Paths({ title, paths, empty }: { title: string; paths: string[]; empty: string }) {
  return (
    <div className="flex flex-col gap-1">
      <p className="text-xs font-medium text-muted-foreground">{title}</p>
      {paths.length ? (
        <ul aria-label={title} className="flex flex-col gap-0.5">
          {paths.map((p) => (
            <Hint text={p}>
              <li key={p} className="truncate font-mono text-xs">
                {p}
              </li>
            </Hint>
          ))}
        </ul>
      ) : (
        <p className="text-xs text-muted-foreground">{empty}</p>
      )}
    </div>
  );
}

/** The quiet Uninstall on an agent row or card; it opens the ask once the plan can be read. */
export function UninstallButton({
  harness,
  disabled,
  why,
  onClick,
}: {
  harness: Pick<Harness, "id">;
  disabled?: boolean;
  /** Why it can't be used now (agents running), shown as its tooltip. */
  why?: string;
  onClick: () => void;
}) {
  // Shows once the plan can be read, never as a stub.
  if (!isRegistered("harness.uninstallPlan")) return null;
  const off = why;
  return (
    <Hint text={off}>
      <Button
        variant="ghost"
        size="sm"
        className="text-muted-foreground"
        data-command="harness.uninstall"
        data-arg={harness.id}
        disabled={disabled || !!off}
        aria-description={off}
        onClick={onClick}
      >
        <Trash2Icon data-icon="inline-start" /> Uninstall
      </Button>
    </Hint>
  );
}
