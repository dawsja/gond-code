import { useEffect, useState } from "react";
import { LockIcon, ServerIcon, ShieldQuestionIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { runCommand } from "@/lib/appState";
import { notify } from "@/lib/notify";
import { errorText } from "@/lib/types";
import { cn } from "@/lib/utils";
import {
  machinesChanged,
  onShowAddMachine,
  onShowEditMachine,
  useMachines,
  useQuestions,
  type Machine,
  type Question,
} from "./machines";

/** A machine's name from what you typed: the host without its user, a name's first part. */
function nameOf(host: string) {
  const h = host.trim().split("@").pop() ?? "";
  if (/^[\d.]+$/.test(h) || h.includes(":")) return h;
  return h.split(".")[0] ?? h;
}

/** As ssh takes a Host name (the Rust side does the same). */
const aliasOf = (name: string) =>
  name
    .trim()
    .replace(/[^A-Za-z0-9._-]+/g, "-")
    .replace(/^-+|-+$/g, "");

/** `user@host:port` → `user@host` and the port. */
function splitAddress(address: string): [string, string] {
  const m = /^(.*?)(?::(\d+))?$/.exec(address);
  return [m?.[1] ?? address, m?.[2] ?? "22"];
}

/**
 * "Add SSH machine": a host from your ssh config, or user@host and a port, and its name, which
 * becomes its Host in Gond's ssh config (`ssh <name>`). Adding it connects, so anything ssh asks
 * comes up at once. The same dialog edits one you added: where it is, not its name.
 */
export function AddMachineDialog({ onAdded }: { onAdded: (m: Machine) => void }) {
  const [open, setOpen] = useState(false);
  const [host, setHost] = useState("");
  const [port, setPort] = useState("22");
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const [editing, setEditing] = useState<Machine>();
  const { machines, sshConfig, refresh } = useMachines();
  useEffect(
    () =>
      onShowAddMachine((h) => {
        setHost(h ?? "");
        setName(h ? nameOf(h) : "");
        setPort("22");
        setError(undefined);
        setEditing(undefined);
        setOpen(true);
        refresh();
      }),
    [refresh],
  );
  useEffect(
    () =>
      onShowEditMachine((m) => {
        const [h, p] = splitAddress(m.address ?? "");
        setHost(h);
        setPort(p);
        setName(m.name);
        setError(undefined);
        setEditing(m);
        setOpen(true);
      }),
    [],
  );
  const added = machines.filter((m) => m.kind === "ssh").map((m) => m.address ?? m.name);
  // A Host your ssh config sets up keeps its name and settings.
  const configured = !editing && sshConfig.includes(host.trim());
  const alias = aliasOf(name || nameOf(host));
  const submit = async () => {
    const h = host.trim();
    if (!h) return setError("A host name, an address, or user@host");
    const p = Number(port) || 22;
    setBusy(true);
    setError(undefined);
    try {
      if (editing) {
        await runCommand<Machine>("machine.edit", {
          id: editing.id,
          host: h,
          ...(p !== 22 ? { port: p } : {}),
        });
        machinesChanged();
        setOpen(false);
        return;
      }
      const m = await runCommand<Machine>("machine.add", {
        host: h,
        ...(p !== 22 ? { port: p } : {}),
        ...(!configured && alias ? { name: alias } : {}),
      });
      machinesChanged();
      setOpen(false);
      onAdded(m);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent
        data-entity="add-machine"
        className="flex max-w-[min(32rem,calc(100vw-3rem))] flex-col gap-4 rounded-3xl p-5 sm:max-w-[min(32rem,calc(100vw-3rem))]"
      >
        <DialogTitle className="text-base">
          {editing ? `Edit ${editing.name}` : "Add SSH machine"}
        </DialogTitle>
        <DialogDescription className="text-[13px] leading-relaxed">
          {editing
            ? "Where Gond reaches it. Terminals already running there keep running."
            : "Gond signs in with your ssh config and keys. Passwords go straight to ssh and are never saved."}
        </DialogDescription>
        {!editing && sshConfig.length + added.length > 0 && (
          <div className="flex flex-col gap-2">
            <span className="text-[12.5px] text-muted-foreground">From your ssh config</span>
            <div className="flex flex-wrap gap-2">
              {sshConfig.map((h) => (
                <button
                  key={h}
                  type="button"
                  onClick={() => {
                    setHost(h);
                    setName(h);
                    setPort("22");
                  }}
                  className={cn(
                    "flex h-7 items-center gap-1.5 rounded-full border px-2.5 text-[12.5px] outline-none hover:bg-hover focus-visible:ring-2 focus-visible:ring-tint",
                    host === h ? "border-tint" : "border-tint-edge",
                  )}
                >
                  <ServerIcon className="size-3.5" />
                  {h}
                </button>
              ))}
            </div>
          </div>
        )}
        <form
          className="flex flex-col gap-3"
          onSubmit={(e) => {
            e.preventDefault();
            void submit();
          }}
        >
          <div className="flex gap-3">
            <label className="flex flex-[3] flex-col gap-1.5">
              <span className="text-[12.5px] text-muted-foreground">Host</span>
              <Input
                autoFocus
                value={host}
                placeholder="user@host, or a Host from your ssh config"
                data-local="machine.host"
                onChange={(e) => {
                  if (!name || name === nameOf(host)) setName(nameOf(e.target.value));
                  setHost(e.target.value);
                }}
                className="font-mono text-[12.5px]"
              />
            </label>
            {!configured && (
              <label className="flex flex-1 flex-col gap-1.5">
                <span className="text-[12.5px] text-muted-foreground">Port</span>
                <Input
                  value={port}
                  inputMode="numeric"
                  onChange={(e) => setPort(e.target.value.replace(/\D/g, ""))}
                  className="font-mono text-[12.5px]"
                />
              </label>
            )}
          </div>
          {editing ? (
            <p className="text-[12.5px] leading-relaxed text-muted-foreground">
              Its name stays <span className="font-mono">{editing.name}</span>. To rename it, remove
              it and add it again.
            </p>
          ) : configured ? (
            <p className="text-[12.5px] leading-relaxed text-muted-foreground">
              Uses <span className="font-mono">{host.trim()}</span>&apos;s settings in your ssh
              config.
            </p>
          ) : (
            <label className="flex flex-col gap-1.5">
              <span className="text-[12.5px] text-muted-foreground">Name</span>
              <Input
                value={name}
                data-local="machine.name"
                onChange={(e) => setName(e.target.value)}
                className="font-mono text-[12.5px]"
              />
              <span className="text-[12px] text-faint">
                {alias ? (
                  <>
                    <span className="font-mono">ssh {alias}</span> reaches it from any terminal too.
                  </>
                ) : (
                  "Letters, digits, '.', '_' and '-'."
                )}
              </span>
            </label>
          )}
          {error && (
            <p role="alert" className="text-[12.5px] text-tone-bad">
              {error}
            </p>
          )}
          <DialogFooter className="mx-0 gap-2 pt-1">
            <Button
              type="button"
              variant="outline"
              className="rounded-full"
              onClick={() => setOpen(false)}
            >
              Cancel
            </Button>
            <Button type="submit" className="rounded-full" disabled={busy}>
              {editing ? "Save" : "Add and connect"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

/**
 * What ssh asks while it signs in (a key's passphrase, a password, whether to trust a new host
 * key), one at a time. The answer goes to ssh; Gond keeps none of it.
 */
export function AskpassDialog() {
  const q = useQuestions()[0];
  return q ? <Ask key={q.id} q={q} /> : null;
}

function Ask({ q }: { q: Question }) {
  const [answer, setAnswer] = useState("");
  const send = (a: string | null) =>
    void runCommand("machine.askpass.answer", {
      id: q.id,
      ...(a !== null ? { answer: a } : {}),
    }).catch((e) => notify.error("Couldn't answer ssh", { description: errorText(e) }));
  const confirm = q.kind === "confirm";
  return (
    <Dialog open onOpenChange={(o) => !o && send(null)}>
      <DialogContent
        data-entity="askpass"
        className="flex max-w-[min(30rem,calc(100vw-3rem))] flex-col gap-4 rounded-3xl p-5 sm:max-w-[min(30rem,calc(100vw-3rem))]"
      >
        <DialogTitle className="flex items-center gap-2.5 text-base">
          {confirm ? (
            <ShieldQuestionIcon className="size-4 text-tone-warn" />
          ) : (
            <LockIcon className="size-4 text-tone-warn" />
          )}
          {q.machineName} asks
        </DialogTitle>
        <DialogDescription className="font-mono text-[12.5px] leading-relaxed whitespace-pre-wrap text-foreground">
          {q.prompt}
        </DialogDescription>
        <form
          className="flex flex-col gap-3"
          onSubmit={(e) => {
            e.preventDefault();
            send(confirm ? "yes" : answer);
          }}
        >
          {!confirm && (
            <Input
              autoFocus
              type="password"
              value={answer}
              data-local="askpass.answer"
              onChange={(e) => setAnswer(e.target.value)}
            />
          )}
          <p className="text-[12px] leading-relaxed text-faint">
            {confirm
              ? "First time connecting. Yes remembers this server, like ssh does."
              : "Goes straight to ssh, never saved. Set up a key to skip this."}
          </p>
          <DialogFooter className="gap-2">
            <Button
              type="button"
              variant="outline"
              className="rounded-full"
              onClick={() => send(confirm ? "no" : null)}
            >
              {confirm ? "No" : "Cancel"}
            </Button>
            <Button type="submit" className="rounded-full" autoFocus={confirm}>
              {confirm ? "Yes, connect" : "OK"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
