import { useEffect, useState } from "react";
import {
  CheckIcon,
  ChevronsUpDownIcon,
  CircleAlertIcon,
  CircleCheckIcon,
  CircleIcon,
  KeyRoundIcon,
  LoaderIcon,
  PlusIcon,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { runCommand } from "@/lib/appState";
import { notify } from "@/lib/notify";
import { errorText } from "@/lib/types";
import { cn } from "@/lib/utils";
import { MachineIcon } from "./MachineIcon";
import {
  machinesChanged,
  onShowCreateKey,
  onShowSetUpKey,
  showCreateKey,
  useKeys,
  useLinks,
  useMachines,
  type Machine,
  type SshKey,
} from "./machines";

type Step = { label: string; sub?: string; state: "todo" | "busy" | "done" | "failed" };

/**
 * Puts `key` on machine `m` and checks it: signs in (ssh may ask for the password one last
 * time), adds the key to the machine's authorized_keys, then signs in with the key alone.
 * `show` gets the steps as they go. Whether it worked.
 */
async function setUp(m: Machine, key: string, show: (s: Step[]) => void): Promise<boolean> {
  const steps: Step[] = [
    { label: `Sign in to ${m.name}`, state: "busy" },
    { label: `Add ${key} to ${m.name}`, sub: "~/.ssh/authorized_keys", state: "todo" },
    { label: "Check it signs in with the key", state: "todo" },
  ];
  const go = async (i: number, run: () => Promise<unknown>) => {
    steps[i] = { ...steps[i], state: "busy" };
    show([...steps]);
    try {
      await run();
      steps[i] = { ...steps[i], state: "done" };
      show([...steps]);
      return true;
    } catch (e) {
      steps[i] = { ...steps[i], state: "failed", sub: errorText(e) };
      show([...steps]);
      return false;
    }
  };
  return (
    (await go(0, () => runCommand("machine.connect", { id: m.id }))) &&
    (await go(1, () => runCommand("machine.key.authorize", { id: m.id, key }))) &&
    (await go(2, () => runCommand("machine.key.check", { id: m.id })))
  );
}

function Steps({ steps }: { steps: Step[] }) {
  return (
    <div className="flex flex-col gap-1 rounded-xl border border-edge px-3 py-2">
      {steps.map((s) => (
        <div key={s.label} className="flex items-start gap-2.5 py-1">
          {s.state === "done" ? (
            <CircleCheckIcon className="mt-px size-4 shrink-0 text-tone-ok" />
          ) : s.state === "busy" ? (
            <LoaderIcon className="mt-px size-4 shrink-0 animate-spin text-tint-ink" />
          ) : s.state === "failed" ? (
            <CircleAlertIcon className="mt-px size-4 shrink-0 text-tone-bad" />
          ) : (
            <CircleIcon className="mt-px size-4 shrink-0 text-faint" />
          )}
          <div className="min-w-0">
            <div className={cn("text-[13px]", s.state === "todo" && "text-muted-foreground")}>
              {s.label}
            </div>
            {s.sub && (
              <div
                className={cn(
                  "text-[12px] break-words",
                  s.state === "failed" ? "text-tone-bad" : "font-mono text-faint",
                )}
              >
                {s.sub}
              </div>
            )}
          </div>
        </div>
      ))}
    </div>
  );
}

const KeyLabel = ({ k }: { k: SshKey }) => (
  <>
    <KeyRoundIcon className="size-3.5 shrink-0 text-muted-foreground" />
    <span className="truncate" title={k.name}>
      {k.name}
    </span>
    <span className="text-[12px] text-faint">{k.kind}</span>
    {k.passphrase && <span className="text-[12px] text-faint">· passphrase</span>}
  </>
);

/** "Sign in to <machine> with a key": pick a key (or make one), and Gond puts it there. */
export function SetUpKeyDialog() {
  const [machineId, setMachineId] = useState<string>();
  const [key, setKey] = useState<string>();
  const [steps, setSteps] = useState<Step[]>();
  const [busy, setBusy] = useState(false);
  const { machines } = useMachines();
  const keys = useKeys();
  useEffect(
    () =>
      onShowSetUpKey((id, k) => {
        setMachineId(id);
        setKey(k);
        setSteps(undefined);
        machinesChanged();
      }),
    [],
  );
  const m = machines.find((x) => x.id === machineId);
  const chosen = keys.find((k) => k.name === (key ?? m?.key)) ?? keys[0];
  const close = () => !busy && setMachineId(undefined);
  const start = async () => {
    if (!m || !chosen) return;
    setBusy(true);
    const ok = await setUp(m, chosen.name, setSteps);
    setBusy(false);
    machinesChanged();
    if (ok) {
      notify.success(`${m.name} signs in with ${chosen.name}`, {
        description: "No password next time.",
      });
      setMachineId(undefined);
    }
  };
  return (
    <Dialog open={!!m} onOpenChange={(o) => !o && close()}>
      <DialogContent
        data-entity="set-up-key"
        className="flex max-w-[min(30rem,calc(100vw-3rem))] flex-col gap-4 rounded-3xl p-5 sm:max-w-[min(30rem,calc(100vw-3rem))]"
      >
        <DialogTitle className="flex items-center gap-2.5 text-base">
          <KeyRoundIcon className="size-4" />
          Sign in to {m?.name} with a key
        </DialogTitle>
        <DialogDescription className="text-[13px] leading-relaxed">
          No more passwords for {m?.name}. Gond copies your public key there. Your private key stays
          on this PC.
        </DialogDescription>
        {keys.length === 0 ? (
          <p className="text-[13px] text-muted-foreground">You have no SSH keys yet.</p>
        ) : (
          <div className="flex flex-col gap-1.5">
            <span className="text-[12.5px] text-muted-foreground">Key</span>
            <DropdownMenu>
              <DropdownMenuTrigger asChild disabled={busy}>
                <button
                  type="button"
                  data-local="key.pick"
                  className="flex h-9 items-center gap-2 rounded-lg border border-edge bg-raised px-3 text-left text-[13px] outline-none focus-visible:ring-2 focus-visible:ring-tint"
                >
                  {chosen && <KeyLabel k={chosen} />}
                  <span className="flex-1" />
                  <ChevronsUpDownIcon className="size-3.5 text-muted-foreground" />
                </button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="start" className="w-80 rounded-xl p-1.5">
                {keys.map((k) => (
                  <DropdownMenuItem key={k.name} onSelect={() => setKey(k.name)}>
                    <KeyLabel k={k} />
                    {k.name === chosen?.name && <CheckIcon className="ml-auto size-3.5" />}
                  </DropdownMenuItem>
                ))}
                <DropdownMenuSeparator />
                <DropdownMenuItem
                  onSelect={() => {
                    setMachineId(undefined);
                    if (m) showCreateKey([m.id]);
                  }}
                >
                  <PlusIcon className="size-3.5" />
                  Create a new key
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        )}
        {steps && <Steps steps={steps} />}
        <DialogFooter className="gap-2">
          <Button variant="outline" className="rounded-full" disabled={busy} onClick={close}>
            Cancel
          </Button>
          {keys.length === 0 ? (
            <Button
              className="rounded-full"
              onClick={() => {
                setMachineId(undefined);
                if (m) showCreateKey([m.id]);
              }}
            >
              Create key
            </Button>
          ) : (
            <Button
              className="rounded-full"
              data-local="key.setUp"
              disabled={busy || !chosen}
              onClick={() => void start()}
            >
              {steps?.some((s) => s.state === "failed") ? "Try again" : "Set up key"}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

/** A name for a new key that isn't taken: ssh's own default first. */
function freeName(keys: SshKey[]) {
  const taken = new Set(keys.map((k) => k.name));
  if (!taken.has("id_ed25519")) return "id_ed25519";
  for (let i = 1; ; i++) {
    const n = i === 1 ? "gond" : `gond-${i}`;
    if (!taken.has(n)) return n;
  }
}

/**
 * "Create SSH key": a name, a type, an optional passphrase, and the machines to set it up on,
 * all in one go. Machines that signed in with a password come ticked.
 */
export function CreateKeyDialog() {
  const [open, setOpen] = useState(false);
  const [name, setName] = useState("");
  const [rsa, setRsa] = useState(false);
  const [passphrase, setPassphrase] = useState("");
  // The machines to set it up on: null until you tick one, the ones asked for and those still
  // on a password until then.
  const [picked, setPicked] = useState<string[] | null>(null);
  const [asked, setAsked] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const [progress, setProgress] = useState<Record<string, Step[]>>({});
  const { machines } = useMachines();
  const keys = useKeys();
  const links = useLinks();
  const ssh = machines.filter((m) => m.kind === "ssh");
  useEffect(
    () =>
      onShowCreateKey((ids) => {
        setOpen(true);
        setName("");
        setRsa(false);
        setPassphrase("");
        setError(undefined);
        setProgress({});
        setAsked(ids);
        setPicked(null);
        machinesChanged();
      }),
    [],
  );
  const ticked = (id: string) =>
    picked ? picked.includes(id) : asked.includes(id) || !!links[id]?.password;
  const tick = (id: string, yes: boolean) => {
    const now = ssh.filter((x) => ticked(x.id)).map((x) => x.id);
    setPicked(yes ? [...now, id] : now.filter((x) => x !== id));
  };
  const shownName = name || freeName(keys);
  const submit = async () => {
    setBusy(true);
    setError(undefined);
    try {
      await runCommand("machine.key.create", {
        name: shownName,
        type: rsa ? "rsa" : "ed25519",
        ...(passphrase ? { passphrase } : {}),
      });
    } catch (e) {
      setBusy(false);
      return setError(errorText(e));
    }
    machinesChanged();
    let all = true;
    for (const m of ssh.filter((x) => ticked(x.id))) {
      const ok = await setUp(m, shownName, (s) => setProgress((p) => ({ ...p, [m.id]: s })));
      all &&= ok;
    }
    setBusy(false);
    machinesChanged();
    notify.success(`Created ${shownName}`, {
      description: all ? undefined : "Some machines didn't take it: see Settings → Machines.",
    });
    if (all) setOpen(false);
  };
  const started = Object.keys(progress).length > 0;
  return (
    <Dialog open={open} onOpenChange={(o) => !busy && setOpen(o)}>
      <DialogContent
        data-entity="create-key"
        className="flex max-w-[min(30rem,calc(100vw-3rem))] flex-col gap-4 rounded-3xl p-5 sm:max-w-[min(30rem,calc(100vw-3rem))]"
      >
        <DialogTitle className="text-base">Create SSH key</DialogTitle>
        <DialogDescription className="sr-only">
          A new key in ~/.ssh, set up on the machines you pick.
        </DialogDescription>
        <form
          className="flex flex-col gap-4"
          onSubmit={(e) => {
            e.preventDefault();
            if (!started) void submit();
          }}
        >
          <label className="flex flex-col gap-1.5">
            <span className="text-[12.5px] text-muted-foreground">Name</span>
            <Input
              autoFocus
              value={name}
              placeholder={freeName(keys)}
              data-local="key.name"
              disabled={busy || started}
              onChange={(e) => setName(e.target.value.replace(/[^A-Za-z0-9._-]/g, ""))}
              className="font-mono text-[12.5px]"
            />
            <span className="text-[12px] text-faint">
              Saved as <span className="font-mono">~/.ssh/{shownName}</span> and{" "}
              <span className="font-mono">{shownName}.pub</span>
            </span>
          </label>
          <div className="flex flex-col gap-1.5">
            <span className="text-[12.5px] text-muted-foreground">Type</span>
            <div className="inline-flex w-fit gap-0.5 rounded-full border border-edge p-[3px]">
              {[
                ["ED25519", false],
                ["RSA 4096", true],
              ].map(([label, value]) => (
                <button
                  key={String(label)}
                  type="button"
                  disabled={busy || started}
                  onClick={() => setRsa(value as boolean)}
                  className={cn(
                    "rounded-full px-3 py-1 text-[12.5px] outline-none focus-visible:ring-2 focus-visible:ring-tint",
                    rsa === value ? "bg-hover text-foreground" : "text-muted-foreground",
                  )}
                >
                  {label}
                </button>
              ))}
            </div>
            <span className="text-[12px] text-faint">
              ED25519 suits almost every server. RSA only for very old ones.
            </span>
          </div>
          <label className="flex flex-col gap-1.5">
            <span className="text-[12.5px] text-muted-foreground">Passphrase (optional)</span>
            <Input
              type="password"
              value={passphrase}
              placeholder="None"
              disabled={busy || started}
              onChange={(e) => setPassphrase(e.target.value)}
            />
            <span className="text-[12px] text-faint">
              With one, Gond asks for it each time it connects.
            </span>
          </label>
          {ssh.length > 0 && (
            <div className="flex flex-col gap-1">
              <span className="text-[12.5px] text-muted-foreground">Set it up on</span>
              {ssh.map((m) =>
                progress[m.id] ? (
                  <div key={m.id} className="flex flex-col gap-1.5 py-1">
                    <span className="flex items-center gap-2 text-[13px]">
                      <MachineIcon id={m.id} size={15} />
                      {m.name}
                    </span>
                    <Steps steps={progress[m.id]} />
                  </div>
                ) : (
                  <button
                    key={m.id}
                    type="button"
                    role="checkbox"
                    aria-checked={ticked(m.id)}
                    data-local="key.on"
                    data-id={m.id}
                    disabled={busy || started}
                    onClick={() => tick(m.id, !ticked(m.id))}
                    className="-mx-2 flex items-center gap-2.5 rounded-lg px-2 py-1.5 text-left text-[13px] outline-none hover:bg-hover focus-visible:ring-2 focus-visible:ring-tint"
                  >
                    <span
                      aria-hidden
                      className={cn(
                        "grid size-4 shrink-0 place-items-center rounded-[5px] border transition-colors",
                        ticked(m.id)
                          ? "border-primary bg-primary text-primary-foreground dark:border-tint dark:bg-tint"
                          : "border-tint-edge bg-transparent",
                      )}
                    >
                      {ticked(m.id) && <CheckIcon className="size-3" strokeWidth={3} />}
                    </span>
                    <MachineIcon id={m.id} size={15} />
                    {m.name}
                    <span className="truncate text-[12px] text-faint">
                      {m.key
                        ? `uses ${m.key}`
                        : links[m.id]?.password
                          ? "signs in with a password"
                          : m.address}
                    </span>
                  </button>
                ),
              )}
            </div>
          )}
          {error && (
            <p role="alert" className="text-[12.5px] text-tone-bad">
              {error}
            </p>
          )}
          <DialogFooter className="gap-2">
            <Button
              type="button"
              variant="outline"
              className="rounded-full"
              disabled={busy}
              onClick={() => setOpen(false)}
            >
              {started ? "Close" : "Cancel"}
            </Button>
            {!started && (
              <Button
                type="submit"
                className="rounded-full"
                disabled={busy}
                data-local="key.create"
              >
                {busy ? "Creating…" : "Create key"}
              </Button>
            )}
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
