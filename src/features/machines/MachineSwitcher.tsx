import { useEffect, useState } from "react";
import { CheckIcon, ChevronsUpDownIcon, PlusIcon, ServerIcon, SettingsIcon } from "lucide-react";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Kbd, KbdGroup } from "@/components/ui/kbd";
import { cn } from "@/lib/utils";
import { LinkDot, MachineIcon } from "./MachineIcon";
import {
  machineName,
  onShowSwitcher,
  showAddMachine,
  useLinks,
  useMachines,
  type Link,
} from "./machines";

/** What a machine's row says on the right: how it's doing. */
function linkWord(id: string, link?: Link) {
  if (!link || link.state === "off")
    return id.startsWith("wsl:") ? "starts when used" : "not connected";
  if (link.state === "up") return "connected";
  if (link.state === "connecting") return link.step ?? "connecting";
  return link.retrying ? "reconnecting" : "can't reach";
}

/**
 * The machine switcher, above Settings: which machine this session opens its terminals on
 * (this PC, a WSL distribution, an SSH machine), with a dot for its connection. Ctrl+Shift+M
 * opens it. Picking one sets this session's; ＋ on a row starts a new session there.
 */
export function MachineSwitcher({
  current,
  onPick,
  onNewSession,
  onManage,
}: {
  /** This session's machine; none is this PC. */
  current?: string;
  onPick: (machine?: string) => void;
  onNewSession: (machine?: string) => void;
  onManage: () => void;
}) {
  const [open, setOpen] = useState(false);
  const { machines, sshConfig, refresh } = useMachines();
  const links = useLinks();
  useEffect(() => onShowSwitcher(() => setOpen(true)), []);
  useEffect(() => {
    if (open) refresh();
  }, [open, refresh]);
  const link = current ? links[current] : undefined;
  const name = current ? machineName(current, machines, links) : "This PC";
  const kind = current?.startsWith("wsl:") ? "WSL" : current?.startsWith("ssh:") ? "SSH" : "";
  const wsl = machines.filter((m) => m.kind === "wsl");
  const ssh = machines.filter((m) => m.kind === "ssh");
  const row = (id: string | undefined, label: string, right: React.ReactNode) => (
    <DropdownMenuItem
      key={id ?? "local"}
      data-local="machine.pick"
      data-arg={id ?? "local"}
      onSelect={() => onPick(id)}
      className="group/row h-8 gap-2.5 rounded-lg px-2.5 text-[13px]"
    >
      <MachineIcon id={id} />
      <span className="min-w-0 flex-1 truncate" title={label}>
        {label}
      </span>
      {right}
      {id !== undefined && (
        <button
          type="button"
          aria-label={`New session on ${label}`}
          title="New session here"
          onPointerDown={(e) => e.stopPropagation()}
          onClick={(e) => {
            e.stopPropagation();
            e.preventDefault();
            setOpen(false);
            onNewSession(id);
          }}
          className="hidden size-5 place-items-center rounded-md bg-raised text-foreground group-hover/row:grid group-data-[highlighted]/row:grid"
        >
          <PlusIcon className="size-3.5" />
        </button>
      )}
      <span className="grid size-4 place-items-center">
        {current === id && <CheckIcon className="size-4 text-tint-ink" />}
      </span>
    </DropdownMenuItem>
  );
  const heading = "px-2.5 pt-2 pb-0.5 text-[11.5px] font-medium text-faint";
  return (
    <DropdownMenu open={open} onOpenChange={setOpen}>
      <DropdownMenuTrigger asChild>
        <button
          type="button"
          data-local="machine.switcher"
          aria-label={`Machine: ${name}`}
          className={cn(
            "flex h-8 w-full items-center gap-2.5 rounded-lg px-2.5 text-left text-[13px] outline-none hover:bg-hover focus-visible:ring-2 focus-visible:ring-tint",
            open && "bg-hover",
          )}
        >
          <MachineIcon id={current} />
          <span className="flex min-w-0 flex-1 items-center gap-1.5">
            <span className="truncate" title={name}>
              {name}
            </span>
            {kind && <span className="text-[11.5px] text-faint">{kind}</span>}
          </span>
          {current && <LinkDot state={link?.state} />}
          <ChevronsUpDownIcon className="size-3.5 text-faint" />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent
        side="top"
        align="start"
        sideOffset={6}
        className="w-80 rounded-xl p-1.5"
        data-entity="machine-switcher"
      >
        <DropdownMenuLabel className={heading}>This session opens terminals on</DropdownMenuLabel>
        {row(undefined, "This PC", null)}
        {wsl.length > 0 && <DropdownMenuLabel className={heading}>WSL</DropdownMenuLabel>}
        {wsl.map((m) =>
          row(
            m.id,
            m.name,
            <span className="flex items-center gap-1.5 text-[12px] text-faint">
              <LinkDot state={links[m.id]?.state} />
              {linkWord(m.id, links[m.id])}
            </span>,
          ),
        )}
        {(ssh.length > 0 || sshConfig.length > 0) && (
          <DropdownMenuLabel className={heading}>SSH</DropdownMenuLabel>
        )}
        {ssh.map((m) =>
          row(
            m.id,
            m.name,
            <span className="flex items-center gap-1.5 text-[12px] text-faint">
              <LinkDot state={links[m.id]?.state} />
              {linkWord(m.id, links[m.id])}
            </span>,
          ),
        )}
        {sshConfig.map((h) => (
          <DropdownMenuItem
            key={h}
            onSelect={() => showAddMachine(h)}
            className="h-8 gap-2.5 rounded-lg px-2.5 text-[13px] text-muted-foreground"
          >
            <ServerIcon className="size-[15px] text-faint" />
            <span className="min-w-0 flex-1 truncate" title={h}>
              {h}
            </span>
            <span className="text-[12px] text-faint">in ~/.ssh/config</span>
          </DropdownMenuItem>
        ))}
        <DropdownMenuSeparator />
        <DropdownMenuItem
          data-local="machine.add"
          onSelect={() => showAddMachine()}
          className="h-8 gap-2.5 rounded-lg px-2.5 text-[13px]"
        >
          <PlusIcon className="size-4 text-muted-foreground" /> Add SSH machine…
        </DropdownMenuItem>
        <DropdownMenuItem onSelect={onManage} className="h-8 gap-2.5 rounded-lg px-2.5 text-[13px]">
          <SettingsIcon className="size-4 text-muted-foreground" /> Manage machines
        </DropdownMenuItem>
        <div className="mt-1 flex items-center gap-2 border-t border-edge px-2.5 pt-2 pb-1 text-[11.5px] text-faint">
          <KbdGroup>
            <Kbd>Ctrl</Kbd>
            <Kbd>Shift</Kbd>
            <Kbd>M</Kbd>
          </KbdGroup>
          opens this
        </div>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
