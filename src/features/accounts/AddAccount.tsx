import { useState } from "react";
import {
  CheckCircle2Icon,
  KeyRoundIcon,
  LockIcon,
  PlayIcon,
  TriangleAlertIcon,
} from "lucide-react";
import { ProviderIcon } from "@/components/brand/provider-icon";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import type { RunCommand } from "@/features/deck/model";
import {
  can,
  endpointOk,
  headerProblem,
  needsKey as keyNeeded,
  optionsFor,
  testLine,
  type AccountOptions,
  type TestResult,
} from "@/lib/accounts";
import { notify } from "@/lib/notify";
import { cn } from "@/lib/utils";
import type { Account, Harness } from "@/lib/harnesses";
import { EndpointOptions } from "./EndpointOptions";
import { errorText } from "@/lib/types";
import { Hint } from "@/components/hint";

type Kind = Account["kind"];

/** `account.kinds { harness }`: what each harness takes. */
export type AccountKinds = { provider: string; kinds: Kind[]; maxOauth?: number | null };

const KIND_LABEL: Record<Kind, string> = {
  oauth: "Sign in",
  apiKey: "API key",
  token: "Token",
  gateway: "API key",
};

const KIND_HINT: Record<Kind, string> = {
  oauth: "Your subscription, through the harness's own sign-in.",
  apiKey:
    "A key from the provider, or from any OpenAI- or Anthropic-compatible endpoint. It goes straight into Windows Credential Manager.",
  token: "A long-lived token the harness makes for you.",
  gateway:
    "Any OpenAI- or Anthropic-compatible address: a gateway, a proxy, Azure, Bedrock or Vertex.",
};

/** The default address each provider's API key talks to, shown as the placeholder. */
const DEFAULT_URL: Record<string, string> = {
  anthropic: "https://api.anthropic.com",
  openai: "https://api.openai.com/v1",
};

/**
 * Add an account: pick the harness, how it
 * signs in, then a label and the key, plus the address for a custom endpoint. The key goes to
 * `account.add` once and is never shown again; the event log keeps it redacted.
 */
export function AddAccount({
  harnesses,
  kindsOf,
  onCommand,
  onClose,
  testDraft,
  signIn,
  initialHarness,
  initialKind,
}: {
  harnesses: Harness[];
  kindsOf: (harness: string) => AccountKinds | undefined;
  onCommand: RunCommand;
  onClose: () => void;
  /** account.test on the draft: tries the key against its endpoint before it's saved.
   *  Undefined until account.test is in the registry. */
  testDraft?: (input: Record<string, unknown>) => Promise<TestResult>;
  /** A new sign-in account: add it, then open its sign-in. Undefined until account.signIn. */
  signIn?: (draft: Record<string, unknown>) => Promise<void>;
  /** Opened from one harness's card: that harness, and the way it was asked for. */
  initialHarness?: string;
  initialKind?: Kind;
}) {
  const [test, setTest] = useState<{ running: boolean; result?: TestResult }>();
  // A result belongs to what was tested: any change clears it.
  const clearTest = () => setTest(undefined);
  // Installed harnesses, plus the one this was opened from (a key can come before the install).
  const installed = harnesses.filter((h) => h.installed || h.id === initialHarness);
  const [harness, setHarness] = useState(initialHarness ?? installed[0]?.id ?? "");
  const kinds = kindsOf(harness);
  // A custom endpoint is an API key with a base URL now, so "gateway" folds
  // into "API key".
  const offered: Kind[] = [
    ...new Set(
      (kinds?.kinds ?? ["apiKey"])
        .map((x: Kind): Kind => (x === "gateway" ? "apiKey" : x))
        // Sign in is offered once it can run, never as a stub.
        .filter((x) => x !== "oauth" || !!signIn),
    ),
  ];
  const [kind, setKind] = useState<Kind>(initialKind ?? "apiKey");
  const k = offered.includes(kind) ? kind : (offered[0] ?? "apiKey");
  const h = installed.find((x) => x.id === harness);
  const [label, setLabel] = useState("");
  const [secret, setSecret] = useState("");
  const [endpoint, setEndpoint] = useState("");
  const [options, setOptions] = useState<AccountOptions>({});
  const api = k === "apiKey";
  const opts = api ? optionsFor(harness, options) : undefined;
  const needsKey = k !== "oauth" && keyNeeded(opts ?? {});
  const rows = (options.headers ?? []).filter((h) => h.name.trim());
  const withUrl = api && can(harness, "endpoint") && !opts?.cloud;
  const url = endpoint.trim();
  const urlOk = !withUrl || url === "" || endpointOk(url);
  const headersOk = !can(harness, "headers") || rows.every((h, _, all) => !headerProblem(h, all));
  const cloudOk =
    !opts?.cloud ||
    (opts.cloud.region.trim() !== "" &&
      (opts.cloud.kind !== "vertex" || opts.cloud.project.trim() !== ""));
  const ready =
    !!harness &&
    label.trim() !== "" &&
    (!needsKey || secret !== "") &&
    urlOk &&
    headersOk &&
    cloudOk;
  const why = !label.trim()
    ? "A label first"
    : needsKey && !secret
      ? "The key first"
      : !urlOk
        ? "The base URL must be https://, or http://localhost for a server on this PC"
        : !headersOk
          ? "Fix the headers first"
          : !cloudOk
            ? "The region (and project) first"
            : undefined;

  const draft = () => ({
    harness,
    kind: k,
    label: label.trim(),
    ...(needsKey && { secret }),
    ...(withUrl && url && { endpoint: url }),
    ...(opts && { options: opts }),
  });
  const runTest = () => {
    if (!testDraft) return;
    const { label: _label, ...input } = draft();
    void _label;
    setTest({ running: true });
    testDraft(input).then(
      (result) => setTest({ running: false, result }),
      (e) =>
        setTest({
          running: false,
          result: { ok: false, error: errorText(e) },
        }),
    );
  };
  const save = () => {
    if (!ready) return;
    onCommand("account.add", draft());
    setSecret("");
    onClose();
  };

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="flex max-h-[calc(100vh-6rem)] max-w-[min(44rem,calc(100vw-3rem))] flex-col gap-5 overflow-y-auto rounded-3xl p-6 sm:max-w-[min(44rem,calc(100vw-3rem))]">
        <div className="flex flex-col gap-1">
          <DialogTitle className="text-lg">Add an account</DialogTitle>
          <DialogDescription>Agents use it; they never see the key.</DialogDescription>
        </div>

        <Field label="Harness">
          <ToggleGroup
            type="single"
            value={harness}
            onValueChange={(v) => v && (clearTest(), setHarness(v))}
            aria-label="Harness"
            data-local="account-draft"
            spacing={2}
            className="flex-wrap"
          >
            {installed.map((x) => (
              <ToggleGroupItem
                key={x.id}
                value={x.id}
                data-local="account-draft"
                className="h-10 gap-2 rounded-full border border-edge px-4 whitespace-nowrap data-[state=on]:border-tint-edge data-[state=on]:bg-tint-wash"
              >
                <ProviderIcon id={x.brand} size={16} />
                {x.label}
              </ToggleGroupItem>
            ))}
          </ToggleGroup>
        </Field>

        <Field label="How it signs in" hint={KIND_HINT[k]}>
          <ToggleGroup
            type="single"
            value={k}
            onValueChange={(v) => v && setKind(v as Kind)}
            aria-label="How it signs in"
            data-local="account-draft"
            spacing={0}
            track
            className="w-fit"
          >
            {offered.map((x) => (
              <ToggleGroupItem
                key={x}
                value={x}
                data-local="account-draft"
                className="h-8 rounded-full px-4 whitespace-nowrap data-[state=on]:bg-hover"
              >
                {KIND_LABEL[x]}
              </ToggleGroupItem>
            ))}
          </ToggleGroup>
        </Field>

        <Field label="Label" hint="How it shows on agents and in the picker">
          <Input
            aria-label="Label"
            data-local="account-draft"
            value={label}
            onChange={(e) => setLabel(e.target.value)}
            placeholder={url ? "Work gateway" : "Personal"}
            className="h-10 rounded-full"
          />
        </Field>

        {withUrl && (
          <Field label="Base URL" hint="Optional: a gateway, a proxy, Azure or a server on this PC">
            <Input
              aria-label="Base URL"
              data-local="account-draft"
              value={endpoint}
              onChange={(e) => (clearTest(), setEndpoint(e.target.value))}
              placeholder={DEFAULT_URL[kinds?.provider ?? ""] ?? "https://"}
              aria-invalid={!urlOk}
              className="h-10 rounded-full font-mono"
            />
          </Field>
        )}

        {needsKey && (
          <Field label={k === "token" ? "Token" : "API key"} hint="Masked, and never shown again">
            <Input
              aria-label={k === "token" ? "Token" : "API key"}
              data-local="account-draft"
              type="password"
              autoComplete="off"
              value={secret}
              onChange={(e) => (clearTest(), setSecret(e.target.value))}
              className="h-10 rounded-full font-mono"
            />
          </Field>
        )}

        {api && (
          <EndpointOptions
            harness={harness}
            value={options}
            onChange={(o) => (clearTest(), setOptions(o))}
          />
        )}

        {k === "oauth" && (
          <p className="text-sm text-muted-foreground">
            {h?.label ?? "The harness"} opens its own sign-in in your browser, in a folder that's
            Gond's, so it never touches your own login.
          </p>
        )}

        {test?.result && (
          <div
            role="status"
            data-entity="account-test"
            data-id={test.result.ok ? "ok" : "failed"}
            className={cn(
              "flex flex-col gap-2 rounded-2xl border p-3 text-sm",
              test.result.ok ? "border-edge" : "border-tone-bad/35 text-tone-bad",
            )}
          >
            <p className="flex items-center gap-2 font-medium">
              {test.result.ok ? (
                <CheckCircle2Icon className="size-4 shrink-0 text-tone-ok" />
              ) : (
                <TriangleAlertIcon className="size-4 shrink-0" />
              )}
              <span className="min-w-0">{testLine(test.result)}</span>
            </p>
            {!!test.result.models?.length && (
              <ul
                className="flex max-h-24 flex-wrap content-start gap-1.5 overflow-y-auto py-0.5"
                data-overflow="scroll"
              >
                {test.result.models.map((m) => (
                  <li key={m}>
                    <code className="rounded-lg bg-raised px-2 py-0.5 font-mono text-xs text-muted-foreground">
                      {m}
                    </code>
                  </li>
                ))}
              </ul>
            )}
          </div>
        )}

        <footer className="flex flex-wrap items-center gap-3">
          <p className="flex min-w-0 flex-1 items-center gap-2 text-xs text-muted-foreground">
            <LockIcon className="size-3.5 shrink-0" />
            Stored in Windows Credential Manager. Agents never see it.
          </p>
          {needsKey && testDraft && (
            <Hint text={ready ? undefined : why}>
              <Button
                variant="outline"
                className="rounded-full"
                data-command="account.test"
                disabled={!ready || test?.running}
                aria-description={ready ? undefined : why}
                onClick={runTest}
              >
                <PlayIcon data-icon="inline-start" />{" "}
                {test?.running ? "Testing…" : "Test connection"}
              </Button>
            </Hint>
          )}
          <Button variant="ghost" data-local="account-cancel" onClick={onClose}>
            Cancel
          </Button>
          {k === "oauth" && signIn ? (
            <Hint text={label.trim() ? undefined : "A label first"}>
              <Button
                className="rounded-full"
                data-command="account.signIn"
                disabled={!harness || !label.trim()}
                aria-description={label.trim() ? undefined : "A label first"}
                onClick={() => {
                  const { label: name, harness: h, kind } = draft();
                  void signIn({ harness: h, kind, label: name }).catch((e) =>
                    notify.error(errorText(e)),
                  );
                  onClose();
                }}
              >
                Sign in
              </Button>
            </Hint>
          ) : (
            <Hint text={why}>
              <Button
                className="rounded-full"
                data-command="account.add"
                disabled={!ready}
                aria-description={why}
                onClick={save}
              >
                <KeyRoundIcon data-icon="inline-start" /> Add account
              </Button>
            </Hint>
          )}
        </footer>
      </DialogContent>
    </Dialog>
  );
}

function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <div role="group" aria-label={label} className="flex min-w-0 flex-col gap-2">
      <p className="text-sm font-medium">
        {label} {hint && <span className="font-normal text-muted-foreground">{hint}</span>}
      </p>
      {children}
    </div>
  );
}
