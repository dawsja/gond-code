import { useState, type ReactNode } from "react";
import {
  CopyIcon,
  EllipsisIcon,
  KeyRoundIcon,
  LockIcon,
  PencilIcon,
  PlusIcon,
  Trash2Icon,
  UnplugIcon,
  XIcon,
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
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { runCommand } from "@/lib/appState";
import { notify } from "@/lib/notify";
import { errorText } from "@/lib/types";
import { LinkDot, MachineIcon } from "./MachineIcon";
import {
  machinesChanged,
  showAddMachine,
  showCreateKey,
  showEditMachine,
  showSetUpKey,
  useKeys,
  useLinks,
  useMachines,
  type Link,
  type Machine,
  type SshKey,
} from "./machines";

/** What a machine's row says under its name. */
function detail(m: Machine, link?: Link) {
  const parts: string[] = [];
  if (m.address) parts.push(m.address);
  const p = link?.probe;
  if (p) parts.push([p.distro || p.os, p.arch].filter(Boolean).join(" "));
  if (!link || link.state === "off")
    parts.push(m.kind === "wsl" ? "Starts with its first terminal" : "Not connected");
  else if (link.state === "up") parts.push("Connected");
  else if (link.state === "connecting") parts.push(link.step ?? "Connecting");
  else parts.push(link.retrying ? "Can't reach it · trying again" : "Can't reach it");
  return parts.join(" · ");
}

const run = (name: string, input: Record<string, unknown>, failed: string) =>
  runCommand(name, input).then(machinesChanged, (e) =>
    notify.error(failed, { description: errorText(e) }),
  );

const Chip = ({ children, warn }: { children: ReactNode; warn?: boolean }) => (
  <span
    className={
      warn
        ? "inline-flex items-center gap-1 rounded-md bg-tone-warn/15 px-1.5 text-[11px] leading-[18px] text-tone-warn"
        : "inline-flex items-center gap-1 rounded-md bg-hover px-1.5 text-[11px] leading-[18px] text-muted-foreground"
    }
  >
    {children}
  </span>
);

const More = ({ label, children }: { label: string; children: ReactNode }) => (
  <DropdownMenu>
    <DropdownMenuTrigger asChild>
      <Button variant="ghost" size="icon-sm" aria-label={`More for ${label}`}>
        <EllipsisIcon />
      </Button>
    </DropdownMenuTrigger>
    <DropdownMenuContent align="end" className="w-60 rounded-xl p-1.5">
      <DropdownMenuLabel className="text-[11.5px] font-medium text-faint">
        {label}
      </DropdownMenuLabel>
      {children}
    </DropdownMenuContent>
  </DropdownMenu>
);

/** A row's add line at the bottom of a section: "+ Add SSH machine". */
const AddRow = ({
  children,
  onClick,
  aside,
}: {
  children: ReactNode;
  onClick: () => void;
  aside?: ReactNode;
}) => (
  <div className="flex items-center gap-2 border-t border-edge px-3.5 py-2.5 first:border-t-0">
    <button
      type="button"
      onClick={onClick}
      className="flex items-center gap-2.5 rounded-md text-[13px] text-muted-foreground outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-tint"
    >
      <PlusIcon className="size-4" />
      {children}
    </button>
    <span className="flex-1" />
    {aside}
  </div>
);

type Ask = { title: string; body: string; yes: string; run: () => Promise<unknown> };

/**
 * Settings → Machines: where terminals run besides this PC, and the SSH keys that sign in to
 * them. WSL distributions are found here; SSH machines you add (Hosts in Gond's ssh config).
 * Each row keeps to its state and one action; the rest is in its ⋯.
 */
export function MachinesSettings() {
  const { machines, sshConfig, configFile } = useMachines();
  const keys = useKeys();
  const links = useLinks();
  const [busy, setBusy] = useState<string>();
  const [ask, setAsk] = useState<Ask>();
  const wsl = machines.filter((m) => m.kind === "wsl");
  const ssh = machines.filter((m) => m.kind === "ssh");
  const nameOf = (id: string) => machines.find((m) => m.id === id)?.name ?? id.slice(4);
  const connect = async (id: string) => {
    setBusy(id);
    try {
      await runCommand("machine.connect", { id });
    } catch (e) {
      notify.error("Couldn't connect", { description: errorText(e) });
    } finally {
      setBusy(undefined);
    }
  };

  const machineRow = (m: Machine) => {
    const link = links[m.id];
    const up = link?.state === "up";
    const password = m.kind === "ssh" && !m.key && !!link?.password;
    return (
      <div
        key={m.id}
        data-entity="machine"
        data-id={m.id}
        className="flex items-center gap-3 border-t border-edge px-3.5 py-3 first:border-t-0"
      >
        <MachineIcon id={m.id} size={18} />
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2 text-[13.5px]">
            {m.name}
            <LinkDot state={link?.state} />
            {m.key && (
              <Chip>
                <KeyRoundIcon className="size-3" />
                {m.key}
              </Chip>
            )}
            {password && (
              <Chip warn>
                <LockIcon className="size-3" />
                Password
              </Chip>
            )}
          </div>
          <div
            className="truncate text-[12px] text-muted-foreground"
            title={link?.error ? `${detail(m, link)}\n${link.error}` : detail(m, link)}
          >
            {detail(m, link)}
          </div>
        </div>
        {password && (
          <Button
            variant="outline"
            size="sm"
            className="rounded-full border-tone-warn/40 text-tone-warn"
            data-local="machine.setUpKey"
            onClick={() => showSetUpKey(m.id)}
          >
            <KeyRoundIcon /> Set up key
          </Button>
        )}
        {up ? (
          <Button
            variant="outline"
            size="sm"
            className="rounded-full"
            onClick={() => void run("machine.disconnect", { id: m.id }, "Couldn't disconnect")}
          >
            Disconnect
          </Button>
        ) : (
          m.kind === "ssh" && (
            <Button
              variant="outline"
              size="sm"
              className="rounded-full"
              disabled={busy === m.id || link?.state === "connecting"}
              onClick={() => void connect(m.id)}
            >
              Connect
            </Button>
          )
        )}
        <More label={m.name}>
          {m.kind === "ssh" && (
            <>
              <DropdownMenuItem onSelect={() => showSetUpKey(m.id)}>
                <KeyRoundIcon />
                {m.key ? "Change key…" : "Set up key…"}
                {m.key && <span className="ml-auto text-[12px] text-faint">{m.key}</span>}
              </DropdownMenuItem>
              {m.address && (
                <DropdownMenuItem onSelect={() => showEditMachine(m)}>
                  <PencilIcon />
                  Edit…
                </DropdownMenuItem>
              )}
              <DropdownMenuSeparator />
            </>
          )}
          <DropdownMenuItem
            onSelect={() =>
              void run(
                "machine.closeAll",
                { id: m.id },
                `Couldn't close the terminals on ${m.name}`,
              )
            }
          >
            <XIcon />
            Close all terminals there
          </DropdownMenuItem>
          {m.kind === "ssh" && (
            <DropdownMenuItem
              variant="destructive"
              onSelect={() => void run("machine.remove", { id: m.id }, `Couldn't remove ${m.name}`)}
            >
              <Trash2Icon />
              Remove from Gond
            </DropdownMenuItem>
          )}
        </More>
      </div>
    );
  };

  const keyRow = (k: SshKey) => {
    const used = k.usedBy.map(nameOf);
    const sub = [
      k.kind,
      k.fingerprint.replace(/^(SHA256:.{4}).*(.{4})$/, "$1…$2"),
      k.passphrase ? "Passphrase" : "",
      used.length ? `Used by ${used.join(", ")}` : k.default ? "Your default key" : "Not used yet",
    ].filter(Boolean);
    return (
      <div
        key={k.name}
        data-entity="ssh-key"
        data-id={k.name}
        className="flex items-center gap-3 border-t border-edge px-3.5 py-3 first:border-t-0"
      >
        <KeyRoundIcon className="size-[18px] shrink-0 text-muted-foreground" />
        <div className="min-w-0 flex-1">
          <div className="text-[13.5px]">{k.name}</div>
          <div className="truncate text-[12px] text-muted-foreground" title={k.fingerprint}>
            {sub.join(" · ")}
          </div>
        </div>
        <Button
          variant="outline"
          size="sm"
          className="rounded-full"
          onClick={() =>
            void navigator.clipboard
              ?.writeText(k.publicKey)
              .then(() => notify.success(`Copied ${k.name}.pub`))
          }
        >
          <CopyIcon /> Copy
        </Button>
        <More label={k.name}>
          <DropdownMenuItem
            onSelect={() =>
              void navigator.clipboard
                ?.writeText(k.publicKey)
                .then(() => notify.success(`Copied ${k.name}.pub`))
            }
          >
            <CopyIcon />
            Copy public key
          </DropdownMenuItem>
          {ssh
            .filter((m) => !k.usedBy.includes(m.id))
            .map((m) => (
              <DropdownMenuItem key={m.id} onSelect={() => showSetUpKey(m.id, k.name)}>
                <KeyRoundIcon />
                Set up on {m.name}…
              </DropdownMenuItem>
            ))}
          {k.usedBy.map((id) => (
            <DropdownMenuItem
              key={id}
              onSelect={() =>
                setAsk({
                  title: `Remove ${k.name} from ${nameOf(id)}?`,
                  body: `Gond takes it out of ${nameOf(id)}'s authorized_keys. ${nameOf(id)} asks for a password next time, unless another key works.`,
                  yes: "Remove key",
                  run: () =>
                    run(
                      "machine.key.revoke",
                      { id, key: k.name },
                      `Couldn't remove ${k.name} from ${nameOf(id)}`,
                    ),
                })
              }
            >
              <UnplugIcon />
              Remove from {nameOf(id)}…
            </DropdownMenuItem>
          ))}
          <DropdownMenuSeparator />
          <DropdownMenuItem
            variant="destructive"
            onSelect={() =>
              setAsk({
                title: `Delete ${k.name}?`,
                body: used.length
                  ? `It goes to the Recycle Bin. ${used.join(", ")} sign${used.length === 1 ? "s" : ""} in with it: without it, you may be locked out.`
                  : "It goes to the Recycle Bin.",
                yes: "Delete key",
                run: () => run("machine.key.delete", { name: k.name }, `Couldn't delete ${k.name}`),
              })
            }
          >
            <Trash2Icon />
            Delete key…
          </DropdownMenuItem>
        </More>
      </div>
    );
  };

  const section = (title: string, hint: ReactNode, rows: ReactNode) => (
    <section className="flex flex-col gap-2">
      <div className="flex items-baseline gap-2.5">
        <h3 className="text-[13px] font-medium">{title}</h3>
        {hint && <span className="text-[12.5px] text-faint">{hint}</span>}
      </div>
      <div className="overflow-hidden rounded-2xl border border-edge">{rows}</div>
    </section>
  );
  const shown = sshConfig.slice(0, 3).join(", ") + (sshConfig.length > 3 ? "…" : "");
  return (
    <div className="flex max-w-3xl flex-col gap-6" data-entity="machines-settings">
      <header>
        <h2 className="text-xl font-semibold">Machines</h2>
        <p className="mt-1.5 text-[14.5px] leading-relaxed text-muted-foreground">
          Run terminals in WSL or on your other computers. Agents there show up in the sidebar and
          keep running when Gond closes.
        </p>
      </header>
      {wsl.length > 0 && section("WSL", null, wsl.map(machineRow))}
      {section(
        "SSH",
        configFile && (
          <>
            Saved in <span className="font-mono">{configFile}</span>
          </>
        ),
        <>
          {ssh.map(machineRow)}
          <AddRow
            onClick={() => showAddMachine()}
            aside={
              sshConfig.length > 0 && (
                <button
                  type="button"
                  className="truncate text-[12px] text-faint outline-none hover:text-muted-foreground focus-visible:ring-2 focus-visible:ring-tint"
                  title={sshConfig.join(", ")}
                  onClick={() => showAddMachine()}
                >
                  {sshConfig.length} more in your ssh config: {shown}
                </button>
              )
            }
          >
            Add SSH machine
          </AddRow>
        </>,
      )}
      {section(
        "SSH keys",
        "Sign in without a password",
        <>
          {keys.map(keyRow)}
          <AddRow onClick={() => showCreateKey()}>Create key</AddRow>
        </>,
      )}
      <Dialog open={!!ask} onOpenChange={(o) => !o && setAsk(undefined)}>
        <DialogContent className="flex max-w-[min(26rem,calc(100vw-3rem))] flex-col gap-4 rounded-3xl p-5 sm:max-w-[min(26rem,calc(100vw-3rem))]">
          <DialogTitle className="text-base">{ask?.title}</DialogTitle>
          <DialogDescription className="text-[13px] leading-relaxed">{ask?.body}</DialogDescription>
          <DialogFooter className="gap-2">
            <Button variant="outline" className="rounded-full" onClick={() => setAsk(undefined)}>
              Cancel
            </Button>
            <Button
              variant="destructive"
              className="rounded-full"
              onClick={() => {
                const a = ask;
                setAsk(undefined);
                void a?.run();
              }}
            >
              {ask?.yes}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
