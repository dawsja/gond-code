import { lazy, Suspense, useEffect, useRef, useState } from "react";
import {
  CheckCircle2Icon,
  ChevronDownIcon,
  ExternalLinkIcon,
  LoaderIcon,
  LockIcon,
  RotateCwIcon,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { runCommand, useAppState } from "@/lib/appState";
import { openExternal } from "@/lib/ipc";
import { errorText } from "@/lib/types";
import { cn } from "@/lib/utils";
import { Hint } from "@/components/hint";

// xterm.js loads only when the details are opened.
const XtermPane = lazy(() => import("@/features/terminal/XtermPane"));

/** `/signins/{jobId}` (AGENT-API account.signIn). */
export type SignInJob = {
  id: string;
  status: "starting" | "waiting" | "needsCode" | "signedIn" | "failed" | "cancelled";
  url?: string;
  code?: string;
  message?: string;
  /** The login's own text so far: what Show details shows, so it's never blank. */
  output?: string;
  ptyId?: string;
};

type Started = { ptyId: string; jobId?: string };

/** The commands a background sign-in sends; swapped in tests. */
export type SignInActions = {
  sendCode: (jobId: string, code: string) => Promise<unknown>;
  cancel: (jobId: string) => Promise<unknown>;
  /** Looks for the agents' own sign-ins again (`account.discover`). */
  discover?: () => Promise<unknown>;
  /** Ends the agent's own CLI once you're done with it. */
  closeTerminal?: (ptyId: string) => Promise<unknown>;
};

const ACTIONS: SignInActions = {
  sendCode: (jobId, code) => runCommand("account.signIn.code", { jobId, code }),
  cancel: (jobId) => runCommand("account.signIn.cancel", { jobId }),
  discover: () => runCommand("account.discover", {}),
  closeTerminal: (ptyId) => runCommand("pty.close", { ptyId }),
};

const SIZE = { cols: 100, rows: 30 };
const STATUS: Record<SignInJob["status"], string> = {
  starting: "Starting the sign-in…",
  waiting: "Approve in your browser… Waiting",
  // Claude always prints "Paste code here if prompted", even when the browser finishes it itself.
  needsCode: "Approve in your browser… Waiting",
  signedIn: "Signed in",
  failed: "The sign-in didn't finish",
  cancelled: "Sign-in cancelled",
};

/**
 * Signing a subscription account in: `account.signIn { accountId }` runs the
 * harness's own login in the background, in that account's own folder, and `/signins/{jobId}`
 * says where it is: the sign-in page and any device code, shown here; a code box when the login
 * asks for one back; then Signed in, or why not, with Retry. The terminal is only a fallback,
 * behind Show details. A backend without jobs (`{ ptyId }` only) gets the terminal, as before.
 */
export function SignIn({
  label,
  harness,
  start,
  onClose,
  actions = ACTIONS,
}: {
  label: string;
  harness: string;
  start: (size: { cols: number; rows: number }) => Promise<Started>;
  onClose: () => void;
  actions?: SignInActions;
}) {
  const [started, setStarted] = useState<Started>();
  const [error, setError] = useState<string>();
  const [attempt, setAttempt] = useState(0);
  const [details, setDetails] = useState(false);
  const startRef = useRef(start);
  useEffect(() => {
    startRef.current = start;
  });
  useEffect(() => {
    let live = true;
    startRef.current(SIZE).then(
      (s) => live && setStarted(s),
      (e) => live && setError(errorText(e)),
    );
    return () => {
      live = false;
    };
  }, [attempt]);
  const job = useAppState((t) =>
    started?.jobId
      ? (t.signins as Record<string, SignInJob> | undefined)?.[started.jobId]
      : undefined,
  );
  const status = error ? "failed" : (job?.status ?? "starting");
  const running = !error && !["signedIn", "failed", "cancelled"].includes(status);
  const retry = () => {
    setError(undefined);
    setStarted(undefined);
    setAttempt((n) => n + 1);
  };
  // An agent with one sign-in per Windows user (agy) runs as you in the terminal: no job.
  const own = !!started && !started.jobId;
  const close = () => {
    if (running && started?.jobId) void actions.cancel(started.jobId).catch(() => undefined);
    // Signed in at the agent's own prompt: Accounts picks it up now.
    if (own) {
      void actions.closeTerminal?.(started.ptyId).catch(() => undefined);
      void actions.discover?.().catch(() => undefined);
    }
    onClose();
  };
  const terminal = started && (!started.jobId || details);

  return (
    <Dialog open onOpenChange={(o) => !o && close()}>
      <DialogContent
        data-entity="account-signin"
        data-id={started?.jobId}
        className={cn(
          "flex max-h-[calc(100vh-3rem)] flex-col gap-4 overflow-y-auto rounded-3xl p-4",
          // An agent's own sign-in screen needs room to draw.
          own
            ? "max-w-[min(56rem,calc(100vw-3rem))] sm:max-w-[min(56rem,calc(100vw-3rem))]"
            : "max-w-[min(40rem,calc(100vw-3rem))] sm:max-w-[min(40rem,calc(100vw-3rem))]",
        )}
      >
        <div className="flex flex-col gap-1">
          <Hint text={`Sign in: ${label}`}>
            <DialogTitle className="truncate text-lg">Sign in: {label}</DialogTitle>
          </Hint>
          <DialogDescription>
            {own
              ? `${harness} keeps one sign-in per Windows user, so this is ${harness} itself: sign in the way it asks, then close this.`
              : `${harness}'s own sign-in, kept in a folder that's Gond's. Your own login isn't touched.`}
          </DialogDescription>
        </div>

        {started?.jobId || error ? (
          <section
            aria-label="Sign-in progress"
            className="flex flex-col gap-3 rounded-lg border border-edge bg-raised/40 p-3"
          >
            <p
              role={status === "failed" ? "alert" : "status"}
              className="flex items-center gap-2 text-sm font-medium"
            >
              {status === "signedIn" ? (
                <CheckCircle2Icon aria-hidden className="size-4 text-tone-ok" />
              ) : running ? (
                <LoaderIcon aria-hidden className="size-4 text-tint motion-safe:animate-spin" />
              ) : null}
              <span className={status === "failed" ? "text-tone-bad" : undefined}>
                {STATUS[status]}
              </span>
            </p>
            {(error || job?.message) && (
              <p className="text-sm text-muted-foreground">{error ?? job?.message}</p>
            )}
            {running && job?.code && (
              <div className="flex flex-col gap-1">
                <p className="text-xs text-muted-foreground">Your code</p>
                <p
                  data-entity="signin-code"
                  className="font-mono text-2xl font-semibold tracking-widest select-all"
                >
                  {job.code}
                </p>
              </div>
            )}
            {running && job?.url && (
              <Hint text={job.url}>
                <Button
                  variant="outline"
                  size="sm"
                  className="w-fit rounded-full"
                  data-local="signin-open-page"
                  onClick={() => void openExternal(job.url!)}
                >
                  <ExternalLinkIcon data-icon="inline-start" /> Open the page again
                </Button>
              </Hint>
            )}
            {running && started?.jobId && job && job.status !== "starting" && (
              <CodeBox onSend={(code) => actions.sendCode(started.jobId!, code)} />
            )}
          </section>
        ) : (
          !started && (
            <p role="status" className="text-sm text-muted-foreground">
              {STATUS.starting}
            </p>
          )
        )}

        {started?.jobId && (
          <Button
            variant="ghost"
            size="sm"
            className="w-fit rounded-full text-muted-foreground"
            data-local="signin-details"
            aria-expanded={details}
            onClick={() => setDetails((d) => !d)}
          >
            <ChevronDownIcon
              data-icon="inline-start"
              className={details ? "rotate-180" : undefined}
            />
            {details ? "Hide details" : "Show details"}
          </Button>
        )}
        {details && job?.output ? (
          <pre
            data-entity="account-signin-output"
            data-overflow="scroll"
            aria-label="Sign-in details"
            className="max-h-72 shrink-0 overflow-auto rounded-lg border border-edge bg-background p-3 font-mono text-xs whitespace-pre-wrap text-muted-foreground"
          >
            {job.output}
          </pre>
        ) : (
          terminal && (
            <div
              data-entity="account-signin-terminal"
              data-overflow="scroll"
              className={cn(
                "shrink-0 overflow-hidden rounded-lg border border-edge bg-background p-2",
                own ? "h-[min(30rem,calc(100vh-16rem))]" : "h-72",
              )}
            >
              <Suspense>
                <XtermPane
                  attach={started.ptyId}
                  fitAttached={own}
                  active
                  onFocus={() => undefined}
                  onExit={started.jobId ? () => undefined : close}
                />
              </Suspense>
            </div>
          )
        )}

        <footer className="flex flex-wrap items-center gap-3">
          <p className="flex min-w-0 flex-1 items-center gap-2 text-xs text-muted-foreground">
            <LockIcon className="size-3.5 shrink-0" />
            {own
              ? `The sign-in stays with ${harness}. Agents use it; they never see it.`
              : "The sign-in stays in this account's folder. Agents use it; they never see it."}
          </p>
          {(status === "failed" || status === "cancelled") && (
            <Button
              variant="outline"
              className="rounded-full"
              data-command="account.signIn"
              onClick={retry}
            >
              <RotateCwIcon data-icon="inline-start" /> Retry
            </Button>
          )}
          {running && started?.jobId ? (
            <Button
              variant="ghost"
              className="rounded-full"
              data-command="account.signIn.cancel"
              onClick={close}
            >
              Cancel
            </Button>
          ) : (
            <Button
              variant={status === "signedIn" ? "default" : "ghost"}
              className="rounded-full"
              data-local="account-signin-close"
              onClick={onClose}
            >
              {status === "signedIn" ? "Done" : "Close"}
            </Button>
          )}
        </footer>
      </DialogContent>
    </Dialog>
  );
}

/** The code a browser shows back, for a login that asks for it (`account.signIn.code`). */
function CodeBox({ onSend }: { onSend: (code: string) => Promise<unknown> }) {
  const [open, setOpen] = useState(false);
  const [code, setCode] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const c = code.trim();
  if (!open)
    return (
      <Button
        variant="link"
        size="sm"
        className="h-auto w-fit p-0 text-muted-foreground"
        data-local="signin-got-code"
        aria-expanded={false}
        onClick={() => setOpen(true)}
      >
        Got a code?
      </Button>
    );
  return (
    <form
      aria-label="Sign-in code"
      className="flex flex-col gap-2"
      onSubmit={(e) => {
        e.preventDefault();
        if (!c) return;
        setBusy(true);
        setError(undefined);
        onSend(c).then(
          () => {
            setBusy(false);
            setCode("");
          },
          (err) => {
            setBusy(false);
            setError(errorText(err));
          },
        );
      }}
    >
      <div className="flex flex-wrap items-center gap-2">
        <Input
          aria-label="Code from your browser"
          autoFocus
          data-local="signin-code"
          autoComplete="off"
          spellCheck={false}
          value={code}
          onChange={(e) => setCode(e.target.value)}
          className="h-9 min-w-48 flex-1 rounded-full font-mono"
        />
        <Hint text={!c ? "Paste the code first" : undefined}>
          <Button
            type="submit"
            size="sm"
            className="rounded-full"
            data-command="account.signIn.code"
            disabled={!c || busy}
            aria-description={!c ? "Paste the code first" : undefined}
          >
            Send code
          </Button>
        </Hint>
      </div>
      {error && (
        <p role="alert" className="text-sm text-tone-bad">
          {error}
        </p>
      )}
    </form>
  );
}
