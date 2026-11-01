import { useEffect, useState } from "react";
import {
  CopyIcon,
  DownloadIcon,
  ExternalLinkIcon,
  KeyRoundIcon,
  LoaderIcon,
  LockIcon,
  PlusIcon,
} from "lucide-react";
import { ToneBadge } from "@/components/gond/badges";
import { ProviderIcon } from "@/components/brand/provider-icon";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import type { RunCommand } from "@/features/deck/model";
import {
  addedLine,
  expiresIn,
  SCOPES,
  errorSentence,
  statusOf,
  type DeviceCode,
  type GithubStatus,
} from "@/lib/github";
import { initialsOf } from "@/lib/github";
import { openExternal } from "@/lib/ipc";
import { cn } from "@/lib/utils";
import { useSetup } from "@/features/setup/useSetup";
import { notify } from "@/lib/notify";
import { errorText } from "@/lib/types";
import { Hint } from "@/components/hint";

const TOKEN_URL = `https://github.com/settings/tokens/new?description=Gond%20Code&scopes=${SCOPES.join(",")}`;

type Props = {
  /** state.github (github.status); undefined until it reaches the UI (live). */
  status?: GithubStatus;
  /** github.login.web: starts `gh auth login --web` and resolves to the one-time code. */
  startWebLogin?: () => Promise<DeviceCode>;
  /** github.login.token: the token goes to `gh auth login --with-token` on stdin; it's redacted. */
  onToken?: (token: string) => Promise<unknown> | void;
  onCommand: RunCommand;
};

/**
 * Settings → Connections: GitHub through the GitHub CLI,
 * with no Gond app and no broker in between. Tokens stay in gh's store; agents never see them.
 */
export function ConnectionsSettings({ status, startWebLogin, onToken, onCommand }: Props) {
  const [adding, setAdding] = useState(false);
  const [confirm, setConfirm] = useState<string>();
  const badge = status && statusOf(status);

  return (
    <div data-slot="connections" className="flex max-w-5xl flex-col gap-6">
      <header>
        <h1 className="text-xl font-semibold">GitHub</h1>
        <p className="mt-1 max-w-3xl text-sm text-muted-foreground">
          Gond Code talks to GitHub through the GitHub CLI on this PC. There's no Gond app and no
          broker in between, and agents never see your token.
        </p>
      </header>

      <section
        aria-label="GitHub"
        data-entity="connection"
        data-id="github"
        className="flex flex-col gap-4 rounded-3xl border border-edge bg-card p-5"
      >
        <div className="flex items-center gap-4">
          <span className="grid size-12 shrink-0 place-items-center rounded-full border border-edge">
            <ProviderIcon id="gh" size={24} />
          </span>
          <div className="min-w-0 flex-1">
            <h2 className="font-semibold">GitHub</h2>
            <Hint text="Git pushes use the active account">
              <p className="truncate text-sm text-muted-foreground">
                {status?.ghInstalled ? (
                  <>
                    via GitHub CLI <code className="font-mono">{status.ghVersion}</code> · Git
                    pushes use the active account
                  </>
                ) : (
                  "via GitHub CLI · Git pushes use the active account"
                )}
              </p>
            </Hint>
          </div>
          {badge && (
            <ToneBadge tone={badge.tone} className="shrink-0">
              {badge.label}
            </ToneBadge>
          )}
        </div>

        {status && !status.ghInstalled && <InstallGh onCommand={onCommand} />}

        {status?.loginError && (
          <p role="alert" className="max-w-[72ch] text-sm text-tone-bad">
            {status.loginError}
          </p>
        )}

        {status && status.accounts.length > 0 && (
          <ul className="flex flex-col divide-y divide-edge rounded-2xl border border-edge">
            {status.accounts.map((a) => (
              <li
                key={a.login}
                data-entity="github-account"
                data-id={a.login}
                className="flex items-center gap-4 px-4 py-3"
              >
                <span
                  aria-hidden
                  className="grid size-10 shrink-0 place-items-center rounded-full bg-raised text-sm font-semibold"
                >
                  {initialsOf(a.login)}
                </span>
                <div className="flex min-w-0 flex-1 flex-col gap-1.5">
                  <p className="flex min-w-0 items-center gap-2">
                    <Hint text={`@${a.login}`}>
                      <span className="truncate font-medium">@{a.login}</span>
                    </Hint>
                    <Hint text={a.host}>
                      <span className="truncate text-sm text-muted-foreground">{a.host}</span>
                    </Hint>
                    {a.active && (
                      <ToneBadge tone="info" icon={false} className="shrink-0">
                        Active
                      </ToneBadge>
                    )}
                  </p>
                  <p className="flex flex-wrap items-center gap-1.5">
                    {a.scopes.map((s) => (
                      <code
                        key={s}
                        className="rounded-md bg-raised px-1.5 py-0.5 font-mono text-xs text-muted-foreground"
                      >
                        {s}
                      </code>
                    ))}
                    <span className="text-xs text-muted-foreground">{addedLine(a)}</span>
                  </p>
                </div>
                {confirm === a.login ? (
                  <div
                    role="group"
                    aria-label={`Sign @${a.login} out?`}
                    className="flex shrink-0 flex-wrap items-center justify-end gap-2"
                  >
                    <Hint text={`Sign @${a.login} out of GitHub CLI on this PC?`}>
                      <span className="text-sm">
                        Sign @{a.login.length > 16 ? `${a.login.slice(0, 16)}…` : a.login} out of
                        GitHub CLI on this PC?
                      </span>
                    </Hint>
                    <Button
                      variant="destructive"
                      size="sm"
                      className="rounded-full"
                      data-command="github.logout"
                      onClick={() => {
                        setConfirm(undefined);
                        onCommand("github.logout", { login: a.login, host: a.host });
                      }}
                    >
                      Sign out
                    </Button>
                    <Button variant="ghost" size="sm" onClick={() => setConfirm(undefined)}>
                      Keep
                    </Button>
                  </div>
                ) : (
                  <div className="flex shrink-0 items-center gap-2">
                    {!a.active && (
                      <Button
                        variant="outline"
                        size="sm"
                        className="rounded-full"
                        data-command="github.switch"
                        onClick={() => onCommand("github.switch", { login: a.login, host: a.host })}
                      >
                        Switch to
                      </Button>
                    )}
                    <Button variant="ghost" size="sm" onClick={() => setConfirm(a.login)}>
                      Disconnect
                    </Button>
                  </div>
                )}
              </li>
            ))}
          </ul>
        )}

        <div className="flex flex-wrap items-center gap-4">
          <Button
            className="rounded-full"
            data-local="github-add-account"
            {...(!status || status.ghInstalled ? {} : NEEDS_GH)}
            onClick={() => setAdding(true)}
          >
            <PlusIcon data-icon="inline-start" /> Add account
          </Button>
          <p className="ml-auto flex items-center gap-2 text-sm text-muted-foreground">
            <LockIcon className="size-3.5 shrink-0" />
            Tokens stay in gh's store and Windows Credential Manager.
          </p>
        </div>
      </section>

      <Tools />

      {adding && (
        <AddAccount
          startWebLogin={startWebLogin}
          onToken={onToken}
          pending={status?.pending}
          loginError={status?.loginError}
          onCommand={onCommand}
          onClose={() => setAdding(false)}
        />
      )}
    </div>
  );
}

function AddAccount({
  startWebLogin,
  onToken,
  pending,
  loginError,
  onCommand,
  onClose,
}: Pick<Props, "startWebLogin" | "onToken" | "onCommand"> &
  Pick<GithubStatus, "pending" | "loginError"> & { onClose: () => void }) {
  const [tab, setTab] = useState<"web" | "token">("web");
  // A sign-in already waiting (state.github.pending) is picked up rather than started again.
  const [device, setDevice] = useState<DeviceCode | undefined>(pending);
  const [error, setError] = useState<ReturnType<typeof errorSentence>>();
  const [token, setToken] = useState("");
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    if (pending || !startWebLogin) return;
    let live = true;
    startWebLogin().then(
      (d) => live && setDevice(d),
      (e) => live && setError(errorSentence(e)),
    );
    return () => {
      live = false;
    };
  }, [pending, startWebLogin, attempt]);
  const retry = () => {
    setError(undefined);
    setDevice(undefined);
    setAttempt((n) => n + 1);
  };
  // The background sign-in failing lands in state.github.loginError.
  const failed = error ?? (loginError ? { text: loginError, ghMissing: false } : undefined);
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, []);

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="flex max-w-[min(48rem,calc(100vw-3rem))] flex-col gap-5 rounded-3xl p-6 sm:max-w-[min(48rem,calc(100vw-3rem))]">
        <DialogTitle className="flex items-center gap-3 text-lg">
          <ProviderIcon id="gh" size={22} /> Add a GitHub account
        </DialogTitle>
        <ToggleGroup
          type="single"
          value={tab}
          onValueChange={(v) => v && setTab(v as typeof tab)}
          aria-label="How to sign in"
          track
          className={cn("grid w-full", onToken ? "grid-cols-2" : "grid-cols-1")}
        >
          <ToggleGroupItem value="web" className="rounded-full data-[state=on]:bg-hover">
            Sign in with browser
          </ToggleGroupItem>
          {/* Use a token shows once it can save one, never as a stub. */}
          {onToken && (
            <ToggleGroupItem value="token" className="rounded-full data-[state=on]:bg-hover">
              Use a token
            </ToggleGroupItem>
          )}
        </ToggleGroup>

        {failed && (
          <div role="alert" className="flex flex-wrap items-center gap-3">
            <p className="max-w-[72ch] min-w-0 flex-1 text-sm text-tone-bad">{failed.text}</p>
            {failed.ghMissing ? (
              <InstallGh onCommand={onCommand} />
            ) : (
              tab === "web" &&
              startWebLogin && (
                <Button
                  className="shrink-0 rounded-full"
                  data-command="github.login.web"
                  onClick={retry}
                >
                  Retry
                </Button>
              )
            )}
          </div>
        )}
        {tab === "web" ? (
          device ? (
            <ol className="flex flex-col gap-5">
              <Step n={1} title="Copy this one-time code">
                <div className="flex items-center gap-4 rounded-2xl border border-edge bg-card px-5 py-4">
                  <code className="flex-1 font-mono text-[30px] font-semibold tracking-[0.3em]">
                    {device.userCode}
                  </code>
                  <Button
                    variant="outline"
                    className="rounded-full"
                    onClick={() => navigator.clipboard?.writeText(device.userCode)}
                  >
                    <CopyIcon data-icon="inline-start" /> Copy
                  </Button>
                </div>
              </Step>
              <Step n={2} title="Paste it on GitHub and approve">
                <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
                  <Button
                    className="rounded-full"
                    onClick={() => void openExternal(device.verificationUri)}
                  >
                    <ExternalLinkIcon data-icon="inline-start" /> Open GitHub in your browser
                  </Button>
                  <span className="text-sm text-muted-foreground">
                    or go to{" "}
                    <code className="font-mono text-foreground">github.com/login/device</code> on
                    any device
                  </span>
                </div>
              </Step>
              <li className="flex items-start gap-4">
                <LoaderIcon
                  aria-hidden
                  className="mt-0.5 size-7 shrink-0 p-1 text-tint motion-safe:animate-spin"
                />
                <div>
                  <p className="font-medium">Waiting for you to approve on GitHub…</p>
                  <p className="text-sm text-muted-foreground">
                    This updates by itself. The code expires in {expiresIn(device.expiresAt, now)}.
                  </p>
                </div>
              </li>
            </ol>
          ) : failed ? null : startWebLogin ? (
            <p className="flex items-center gap-2 text-sm text-muted-foreground">
              <LoaderIcon aria-hidden className="size-4 motion-safe:animate-spin" /> Asking GitHub
              for a code…
            </p>
          ) : (
            <p className="text-sm text-muted-foreground">
              Signing in through the GitHub CLI is not available yet.
            </p>
          )
        ) : (
          <form
            className="flex flex-col gap-3"
            onSubmit={(e) => {
              e.preventDefault();
              if (!onToken || !token) return;
              setError(undefined);
              Promise.resolve(onToken(token)).then(onClose, (err) => setError(errorSentence(err)));
              setToken("");
            }}
          >
            <label className="flex flex-col gap-2 text-sm font-medium">
              Personal access token
              <Input
                type="password"
                autoComplete="off"
                value={token}
                onChange={(e) => setToken(e.target.value)}
                placeholder="ghp_…"
                className="font-mono"
              />
            </label>
            <p className="text-sm text-muted-foreground">
              It goes straight to the GitHub CLI and isn't shown again.{" "}
              <button
                type="button"
                className="text-tint-ink underline-offset-4 hover:underline"
                onClick={() => void openExternal(TOKEN_URL)}
              >
                Create a token with the right scopes
              </button>
            </p>
            <Button type="submit" className="w-fit rounded-full" disabled={!token}>
              Save token
            </Button>
          </form>
        )}

        <p className="flex items-start gap-2 rounded-2xl bg-card px-4 py-3 text-sm text-muted-foreground">
          <KeyRoundIcon className="mt-0.5 size-4 shrink-0" />
          <span>
            Asks for{" "}
            {SCOPES.map((s, i) => (
              <span key={s}>
                {i > 0 && (i === SCOPES.length - 1 ? " and " : ", ")}
                <code className="font-mono text-foreground">{s}</code>
              </span>
            ))}
            : enough for your agents to push and open pull requests.
          </span>
        </p>
        <footer className="flex flex-wrap items-center gap-4">
          <p className="font-mono text-xs text-faint">
            {tab === "web"
              ? "Runs gh auth login --web, then gh auth setup-git"
              : "Runs gh auth login --with-token, then gh auth setup-git"}
          </p>
          <Button variant="ghost" className="ml-auto" onClick={onClose}>
            Cancel
          </Button>
        </footer>
      </DialogContent>
    </Dialog>
  );
}

const NEEDS_GH = {
  disabled: true,
  title: "Install the GitHub CLI first",
  "aria-description": "Install the GitHub CLI first",
};

/** gh_missing: the UI offers prereq.install for "gh", the flow. */
function InstallGh({ onCommand }: { onCommand: RunCommand }) {
  return (
    <Button
      className="w-fit shrink-0 rounded-full"
      data-command="prereq.install"
      onClick={() => onCommand("prereq.install", { prereqs: ["gh"] })}
    >
      Install GitHub CLI
    </Button>
  );
}

function Step({ n, title, children }: { n: number; title: string; children: React.ReactNode }) {
  return (
    <li className="flex items-start gap-4">
      <span
        aria-hidden
        className={cn(
          "grid size-7 shrink-0 place-items-center rounded-full bg-primary text-sm font-semibold text-primary-foreground",
        )}
      >
        {n}
      </span>
      <div className="flex min-w-0 flex-1 flex-col gap-3">
        <p className="font-medium">{title}</p>
        {children}
      </div>
    </li>
  );
}

/**
 * Git and the GitHub CLI (optional; they live here). Agents use Git to track their changes and gh to push and open pull requests. Each shows
 * its version, or Install (prereq.install; the set-up toast follows the job).
 */
function Tools() {
  const setup = useSetup();
  if (!setup.live || !setup.prereqs?.length) return null;
  return (
    <section
      aria-label="Git and the GitHub CLI"
      className="flex flex-col gap-3 rounded-3xl border border-edge bg-card p-5"
    >
      <div>
        <h2 className="font-semibold">Git and the GitHub CLI</h2>
        <p className="text-sm text-muted-foreground">
          Optional. Your agents use Git to track their changes and the GitHub CLI to push and open
          pull requests.
        </p>
      </div>
      <ul className="flex flex-col divide-y divide-border">
        {setup.prereqs.map((p) => (
          <li
            key={p.id}
            data-entity="prereq"
            data-id={p.id}
            className="flex items-center gap-3 py-2"
          >
            <span className="min-w-0 flex-1 font-medium">{p.name}</span>
            {p.installed ? (
              <ToneBadge tone="ok">{p.version ?? "Installed"}</ToneBadge>
            ) : (
              <Button
                size="sm"
                variant="outline"
                className="rounded-full"
                data-command="prereq.install"
                data-arg={p.id}
                disabled={!!setup.job}
                onClick={() =>
                  void setup
                    .install({ prereqs: [p.id] })
                    .catch((e) =>
                      notify.error(`Couldn't install ${p.name}`, { description: errorText(e) }),
                    )
                }
              >
                <DownloadIcon data-icon="inline-start" /> Install
              </Button>
            )}
          </li>
        ))}
      </ul>
    </section>
  );
}
