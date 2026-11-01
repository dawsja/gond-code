import { Fragment, useState } from "react";
import {
  CheckIcon,
  ChevronDownIcon,
  FolderIcon,
  FolderOpenIcon,
  KeyRoundIcon,
  SquareTerminalIcon,
} from "lucide-react";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Dialog as DialogPrimitive } from "radix-ui";
import { ProviderIcon } from "@/components/brand/provider-icon";
import type { Account, Level, ModelInfo } from "@/lib/harnesses";
import { LEVEL_LABEL } from "@/lib/harnesses";
import { runCommand } from "@/lib/appState";
import { pickFolder } from "@/lib/ipc";
import { notify } from "@/lib/notify";
import { errorText, type AgentInfo, type Project } from "@/lib/types";
import { cn } from "@/lib/utils";
import { accountLine } from "./accountLine";
import { Hint } from "@/components/hint";

/** Whose accounts each built-in agent signs in with (an API key works in any agent of its provider). */
const PROVIDERS: Record<string, string[]> = {
  claude: ["anthropic"],
  codex: ["openai"],
  agy: ["google"],
  grok: ["xai"],
  copilot: ["github"],
  cursor: ["cursor"],
};

/** The agent's accounts: its own sign-ins, then keys for its provider; the default first. */
function accountsFor(agentId: string, accounts: Account[]) {
  return accounts
    .filter(
      (a) =>
        a.harness === agentId || (!a.harness && (PROVIDERS[agentId] ?? []).includes(a.provider)),
    )
    .sort((a, b) => Number(!!b.isDefault) - Number(!!a.isDefault));
}

/** What the picker starts: an agent (or a plain terminal) and where it goes. */
export type Start = {
  agentId: string | "shell";
  projectId: string;
  accountId?: string;
  model?: string;
  thinking?: Level;
  place: "right" | "down";
  /** A key pasted here: saved (in Credential Manager) as the agent's account before it starts. */
  apiKey?: string;
};

/** The "Sign in as" choice that asks for a key. */
const NEW_KEY = "+key";

const field =
  "flex h-9 w-full items-center rounded-[10px] border border-edge bg-raised px-2.5 text-[12.5px] outline-none focus-visible:ring-2 focus-visible:ring-tint";
const label = "text-[10.5px] font-medium tracking-[0.06em] text-faint uppercase";

/**
 * New agent: pick an installed agent, how it signs in (your subscription or an
 * API key), the folder, model and mode, and where it opens; it starts the agent's own CLI there.
 */
export function NewAgent(props: Props) {
  // The form mounts each time it opens, so it starts from where you opened it.
  return (
    <DialogPrimitive.Root open={props.open} onOpenChange={props.onOpenChange}>
      <DialogPrimitive.Portal>{props.open && <Form {...props} />}</DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
}

type Props = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  agents: AgentInfo[];
  accounts: Account[];
  models: Record<string, ModelInfo[]>;
  projects: Project[];
  /** Where it opens from: the folder you're in, the side of the pane you split, a default agent. */
  initial: Partial<Start>;
  /** Whether the chosen folder has panes already (then it asks right or down). */
  hasPanes: (projectId: string) => boolean;
  onStart: (s: Start) => void;
};

function Form({
  onOpenChange,
  agents,
  accounts,
  models,
  projects,
  initial,
  hasPanes,
  onStart,
}: Props) {
  const installed = agents.filter((a) => a.installed);
  const [pick, setPick] = useState<Start>(() => ({
    agentId:
      initial.agentId && installed.some((a) => a.id === initial.agentId)
        ? initial.agentId
        : (installed[0]?.id ?? "shell"),
    projectId: initial.projectId ?? projects[0]?.id ?? "",
    place: initial.place ?? "right",
  }));
  const set = (p: Partial<Start>) => setPick((s) => ({ ...s, ...p }));
  const shell = pick.agentId === "shell";
  const agent = installed.find((a) => a.id === pick.agentId);
  const mine = shell ? [] : accountsFor(pick.agentId, accounts);
  const list = shell ? [] : (models[pick.agentId] ?? []);
  const ready = !!pick.projectId;
  // Any folder on this PC: Browse picks it and it's ready here at once.
  const [added, setAdded] = useState<Project[]>([]);
  const folders = [...projects, ...added.filter((a) => !projects.some((p) => p.id === a.id))];
  const browse = async () => {
    const path = await pickFolder();
    if (!path) return;
    try {
      const p = await runCommand<Project>("project.add", { path });
      setAdded((a) => [...a, p]);
      set({ projectId: p.id });
    } catch (e) {
      notify.error("Couldn't use that folder", { description: errorText(e) });
    }
  };
  const pasting = pick.accountId === NEW_KEY;
  const start = () => {
    if (!ready || (pasting && !pick.apiKey?.trim())) return;
    onStart(
      pasting
        ? { ...pick, accountId: undefined, apiKey: pick.apiKey?.trim() }
        : { ...pick, accountId: pick.accountId ?? mine[0]?.id, apiKey: undefined },
    );
    onOpenChange(false);
  };

  return (
    <>
      <DialogPrimitive.Overlay className="fixed inset-0 z-50 bg-black/50" />
      <DialogPrimitive.Content
        aria-describedby={undefined}
        onKeyDown={(e) => {
          // Enter starts it, unless a menu has the key (it picks there).
          if (e.key === "Enter" && !(e.target as HTMLElement).closest("[role=menu]")) {
            e.preventDefault();
            start();
          }
        }}
        className="fixed top-[14vh] left-1/2 z-50 flex w-[600px] -translate-x-1/2 overflow-hidden rounded-2xl border border-edge bg-card shadow-[0_24px_60px_rgba(0,0,0,.7)] outline-none"
      >
        <DialogPrimitive.Title className="sr-only">New agent</DialogPrimitive.Title>
        <div className="flex w-[220px] shrink-0 flex-col gap-0.5 border-r border-edge p-1.5">
          <p className={cn(label, "px-2.5 pt-2 pb-1")}>Installed agents</p>
          {installed.map((a) => (
            <button
              key={a.id}
              type="button"
              onClick={() =>
                set({
                  agentId: a.id,
                  accountId: undefined,
                  model: undefined,
                  thinking: undefined,
                })
              }
              className={cn(
                "flex h-12 items-center gap-2.5 rounded-[10px] px-2.5 text-left outline-none hover:bg-hover focus-visible:ring-2 focus-visible:ring-tint",
                pick.agentId === a.id && "bg-hover",
              )}
            >
              <ProviderIcon id={a.provider ?? a.id} size={17} />
              <span className="flex min-w-0 flex-col">
                <Hint text={a.name}>
                  <span className="truncate text-[13px]">{a.name}</span>
                </Hint>
                {a.version && <span className="text-[11px] text-faint">{a.version}</span>}
              </span>
            </button>
          ))}
          {installed.length === 0 && (
            <p className="px-2.5 py-2 text-[12px] text-muted-foreground">
              No agents installed yet: add one in Settings → Agents.
            </p>
          )}
          <div className="mx-2 my-1 h-px bg-edge" />
          <button
            type="button"
            onClick={() => set({ agentId: "shell" })}
            className={cn(
              "flex h-9 items-center gap-2.5 rounded-[10px] px-2.5 text-[12.5px] text-muted-foreground outline-none hover:bg-hover focus-visible:ring-2 focus-visible:ring-tint",
              shell && "bg-hover text-foreground",
            )}
          >
            <SquareTerminalIcon className="size-4" /> Plain terminal
          </button>
        </div>
        <div className="flex min-w-0 flex-1 flex-col gap-3.5 p-4">
          {!shell && (
            <label className="flex flex-col gap-1.5">
              <span className={label}>Sign in as</span>
              <Choice
                label="Sign in as"
                value={pick.accountId ?? mine[0]?.id ?? ""}
                onChange={(id) => set({ accountId: id || undefined })}
                options={[
                  ...mine.map((a) => ({
                    id: a.id,
                    icon: <ProviderIcon id={a.provider} size={14} />,
                    label: accountLine(a),
                    note: a.kind === "oauth" ? "Subscription" : "API key",
                  })),
                  ...(mine.length === 0
                    ? [{ id: "", label: `${agent?.name ?? "Its"} own sign-in` }]
                    : []),
                  {
                    id: NEW_KEY,
                    icon: <KeyRoundIcon className="size-3.5 text-muted-foreground" />,
                    label: "Paste an API key…",
                    divided: true,
                  },
                ]}
              />
              {pasting && (
                <input
                  type="password"
                  autoFocus
                  autoComplete="off"
                  spellCheck={false}
                  aria-label="API key"
                  placeholder="sk-…"
                  value={pick.apiKey ?? ""}
                  onChange={(e) => set({ apiKey: e.target.value })}
                  className={cn(field, "font-mono")}
                />
              )}
            </label>
          )}
          <label className="flex flex-col gap-1.5">
            <span className={label}>Folder</span>
            <Choice
              label="Folder"
              value={pick.projectId}
              placeholder="Pick a folder"
              onChange={(id) => (id === BROWSE ? void browse() : set({ projectId: id }))}
              options={[
                ...folders.map((p) => ({
                  id: p.id,
                  icon: <FolderIcon className="size-3.5 text-muted-foreground" />,
                  label: p.name,
                  note: p.path,
                })),
                {
                  id: BROWSE,
                  icon: <FolderOpenIcon className="size-3.5 text-muted-foreground" />,
                  label: "Browse for a folder…",
                  divided: folders.length > 0,
                },
              ]}
            />
          </label>
          {!shell && (
            <label className="flex flex-col gap-1.5">
              <span className={label}>Model</span>
              <ModelMenu
                agent={agent}
                models={list}
                model={pick.model}
                thinking={pick.thinking}
                onPick={(model, thinking) => set({ model, thinking })}
              />
            </label>
          )}
          {hasPanes(pick.projectId) && (
            <div className="flex flex-col gap-1.5">
              <span className={label}>Open in</span>
              <Segments
                value={pick.place}
                onChange={(place) => set({ place })}
                options={[
                  { id: "right", label: "Split right" },
                  { id: "down", label: "Split down" },
                ]}
              />
            </div>
          )}
          <div className="mt-1 flex items-center justify-end gap-2">
            <DialogPrimitive.Close className="h-8 rounded-full px-3 text-[12.5px] text-muted-foreground outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-tint">
              Cancel
            </DialogPrimitive.Close>
            <button
              type="button"
              disabled={!ready}
              onClick={start}
              className="h-8 rounded-full bg-tint px-4 text-[12.5px] font-semibold text-white outline-none disabled:opacity-40 focus-visible:ring-2 focus-visible:ring-tint-ink"
            >
              Start {shell ? "terminal" : (agent?.name ?? "")}
            </button>
          </div>
        </div>
      </DialogPrimitive.Content>
    </>
  );
}

function Segments<T extends string>({
  value,
  onChange,
  options,
}: {
  value: T;
  onChange: (v: T) => void;
  options: { id: T; label: string; className?: string }[];
}) {
  return (
    <div
      role="radiogroup"
      className="flex h-9 gap-1 rounded-[10px] border border-edge bg-raised p-[3px]"
    >
      {options.map((o) => (
        <button
          key={o.id}
          type="button"
          role="radio"
          aria-checked={value === o.id}
          onClick={() => onChange(o.id)}
          className={cn(
            "flex-1 rounded-[7px] text-[12px] text-muted-foreground outline-none focus-visible:ring-2 focus-visible:ring-tint",
            value === o.id && "bg-hover text-foreground",
            o.className,
          )}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

/** "Browse for a folder…" in the Folder menu. */
const BROWSE = "+browse";

type Option = {
  id: string;
  label: string;
  icon?: React.ReactNode;
  /** Beside the label, quieter (the plan, the folder's path). */
  note?: string;
  /** A line above it, setting it apart from the list. */
  divided?: boolean;
};

const trigger =
  "flex h-9 w-full min-w-0 items-center gap-2 rounded-[10px] border border-edge bg-raised px-2.5 text-left text-[12.5px] outline-none hover:bg-hover focus-visible:ring-2 focus-visible:ring-tint data-[state=open]:border-tint-edge";

/** One choice from a list, in the shadcn menu the app uses everywhere (not the browser's). */
function Choice({
  label,
  value,
  options,
  placeholder,
  onChange,
}: {
  label: string;
  value: string;
  options: Option[];
  placeholder?: string;
  onChange: (id: string) => void;
}) {
  const on = options.find((o) => o.id === value);
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        aria-label={`${label}: ${on?.label ?? placeholder ?? ""}`}
        className={trigger}
      >
        {on?.icon}
        <Hint text={on?.note ?? on?.label}>
          <span className="min-w-0 flex-1 truncate">
            {on?.label ?? <span className="text-muted-foreground">{placeholder}</span>}
          </span>
        </Hint>
        {on?.note && (
          <span className="shrink-0 text-[11.5px] text-faint">
            {on.note.length > 28 ? "" : on.note}
          </span>
        )}
        <ChevronDownIcon className="size-3.5 shrink-0 text-muted-foreground" />
      </DropdownMenuTrigger>
      <DropdownMenuContent
        align="start"
        collisionPadding={12}
        className="max-h-[min(22rem,var(--radix-dropdown-menu-content-available-height))] w-(--radix-dropdown-menu-trigger-width) min-w-64 overflow-y-auto rounded-2xl p-1.5 flex flex-col gap-0.5"
      >
        {options.map((o) => (
          <Fragment key={o.id || "none"}>
            {o.divided && <DropdownMenuSeparator />}
            <DropdownMenuItem
              onSelect={() => onChange(o.id)}
              className={cn("gap-2.5 rounded-xl px-2 py-1.5", o.id === value && "bg-tint-wash")}
            >
              {o.icon}
              <span className="flex min-w-0 flex-1 flex-col">
                <Hint text={o.label}>
                  <span className="truncate font-medium">{o.label}</span>
                </Hint>
                {o.note && (
                  <Hint text={o.note}>
                    <span className="truncate text-[11.5px] text-faint">{o.note}</span>
                  </Hint>
                )}
              </span>
              {o.id === value && <CheckIcon className="size-3.5 shrink-0 text-tint-ink" />}
            </DropdownMenuItem>
          </Fragment>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/**
 * The model, as the chat's menu had it: each model with its brand icon; one
 * that thinks has a ›, opening its thinking levels beside it, and you pick the model by picking a
 * level. One that doesn't think is picked by clicking it.
 */
function ModelMenu({
  agent,
  models,
  model,
  thinking,
  onPick,
}: {
  agent?: AgentInfo;
  models: ModelInfo[];
  model?: string;
  thinking?: Level;
  onPick: (model: string | undefined, thinking: Level | undefined) => void;
}) {
  const m = models.find((x) => x.id === model);
  const level = m?.thinking ? (thinking ?? m.thinking.default) : undefined;
  const brand = agent?.provider ?? agent?.id ?? "";
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        aria-label={`Model: ${m?.label ?? "Default"}${level ? `, thinking ${LEVEL_LABEL[level]}` : ""}`}
        className={trigger}
      >
        <ProviderIcon id={m?.provider ?? brand} size={14} />
        <span className="min-w-0 flex-1 truncate">{m?.label ?? "Default model"}</span>
        {level && (
          <span className="shrink-0 rounded-[4px] bg-hover px-1.5 text-[11px] text-muted-foreground">
            {LEVEL_LABEL[level]}
          </span>
        )}
        <ChevronDownIcon className="size-3.5 shrink-0 text-muted-foreground" />
      </DropdownMenuTrigger>
      <DropdownMenuContent
        align="start"
        collisionPadding={12}
        className="max-h-[min(24rem,var(--radix-dropdown-menu-content-available-height))] w-(--radix-dropdown-menu-trigger-width) min-w-72 overflow-y-auto rounded-2xl p-1.5 flex flex-col gap-0.5"
      >
        <DropdownMenuItem
          onSelect={() => onPick(undefined, undefined)}
          className={cn("gap-2.5 rounded-xl px-2 py-1.5", !m && "bg-tint-wash")}
        >
          <ProviderIcon id={brand} size={16} />
          <span className="min-w-0 flex-1 truncate font-medium">Default model</span>
          {!m && <CheckIcon className="size-3.5 shrink-0 text-tint-ink" />}
        </DropdownMenuItem>
        {models.map((x) => {
          const on = x.id === m?.id;
          const row = (
            <>
              <ProviderIcon id={x.provider ?? brand} size={16} />
              <span className="flex min-w-0 flex-1 items-baseline gap-2">
                <span className="truncate font-medium">{x.label}</span>
                {x.goodFor && (
                  <span className="truncate text-[11.5px] text-faint">{x.goodFor}</span>
                )}
              </span>
              {on && <CheckIcon className="size-3.5 shrink-0 text-tint-ink" />}
            </>
          );
          if (!x.thinking)
            return (
              <DropdownMenuItem
                key={x.id}
                data-entity="model"
                data-id={x.id}
                onSelect={() => onPick(x.id, undefined)}
                className={cn("gap-2.5 rounded-xl px-2 py-1.5", on && "bg-tint-wash")}
              >
                {row}
              </DropdownMenuItem>
            );
          return (
            <DropdownMenuSub key={x.id}>
              <DropdownMenuSubTrigger
                data-entity="model"
                data-id={x.id}
                className={cn(
                  "gap-2.5 rounded-xl px-2 py-1.5 data-open:bg-hover [&>svg:last-child]:ml-0 [&>svg:last-child]:size-3.5 [&>svg:last-child]:text-faint",
                  on && "bg-tint-wash data-open:bg-tint-wash",
                )}
              >
                {row}
              </DropdownMenuSubTrigger>
              <DropdownMenuSubContent
                sideOffset={8}
                collisionPadding={12}
                aria-label={`Thinking for ${x.label}`}
                className="w-40 rounded-2xl p-1.5 flex flex-col gap-0.5"
              >
                {x.thinking.levels.map((l) => (
                  <DropdownMenuItem
                    key={l}
                    data-entity="thinking"
                    data-id={l}
                    onSelect={() => onPick(x.id, l)}
                    className={cn(
                      "gap-2.5 rounded-xl px-2.5 py-2",
                      on && l === level && "bg-tint-wash",
                    )}
                  >
                    <span className="font-medium">{LEVEL_LABEL[l]}</span>
                    {on && l === level && (
                      <CheckIcon className="ml-auto size-3.5 shrink-0 text-tint-ink" />
                    )}
                  </DropdownMenuItem>
                ))}
              </DropdownMenuSubContent>
            </DropdownMenuSub>
          );
        })}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
