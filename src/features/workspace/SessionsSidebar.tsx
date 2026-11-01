import { useEffect, useState } from "react";
import {
  CheckIcon,
  CircleAlertIcon,
  CircleIcon,
  LoaderIcon,
  PencilIcon,
  PlusIcon,
  SettingsIcon,
  Trash2Icon,
  XIcon,
} from "lucide-react";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { ProviderIcon } from "@/components/brand/provider-icon";
import { AppUpdate } from "@/features/shell/AppUpdate";
import type { AgentInfo, Project } from "@/lib/types";
import { cn } from "@/lib/utils";
import { panes, type Session, type Pane } from "./layout";
import { ago, STATUS_LABEL, type PaneStatus, type Status } from "./status";
import { Hint } from "@/components/hint";

/** Agents by what they're doing, the ones that want you first. */
const ORDER: Status[] = ["needs", "working", "done", "idle", "exited"];

const TONE: Record<Status, string> = {
  needs: "text-tone-warn",
  working: "text-tint-ink",
  done: "text-tone-ok",
  idle: "text-muted-foreground",
  exited: "text-tone-bad",
};

function StatusIcon({ status }: { status: Status }) {
  const c = "size-3.5 shrink-0";
  if (status === "working") return <LoaderIcon className={cn(c, "animate-spin text-tint-ink")} />;
  if (status === "needs") return <CircleAlertIcon className={cn(c, "text-tone-warn")} />;
  if (status === "done") return <CheckIcon className={cn(c, "text-tone-ok")} />;
  if (status === "exited") return <XIcon className={cn(c, "text-tone-bad")} />;
  return <CircleIcon className={cn("size-2.5 shrink-0 text-faint")} />;
}

/**
 * The sidebar: your sessions, by name
 * (+ adds one); then every agent (+ starts one, as Ctrl T does) in every session, grouped by what it's doing (needs you, working,
 * done, idle) with how long it's been at it. Settings below.
 */
export function SessionsSidebar({
  sessions,
  current,
  focusedPane,
  projects,
  settings,
  agents,
  statuses,
  renaming,
  onNewAgent,
  onSelectSession,
  onAddSession,
  onRenameSession,
  onRenamedSession,
  onRemoveSession,
  onSelectPane,
  onSettings,
  onUpdates,
}: {
  sessions: Session[];
  current?: string;
  focusedPane?: string;
  /** For each agent's folder name. */
  projects: Project[];
  /** Whether Settings is showing. */
  settings: boolean;
  agents: AgentInfo[];
  statuses: Record<string, PaneStatus>;
  /** The session being renamed. */
  renaming?: string;
  onNewAgent: () => void;
  onSelectSession: (id: string) => void;
  onAddSession: () => void;
  onRenameSession: (id: string) => void;
  onRenamedSession: (id: string, name: string) => void;
  onRemoveSession: (id: string) => void;
  onSelectPane: (paneId: string) => void;
  onSettings: () => void;
  /** Settings → Updates, from the sidebar's update notice. */
  onUpdates: () => void;
}) {
  // "3m" moves on by itself.
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const t = window.setInterval(() => setNow(Date.now()), 15_000);
    return () => window.clearInterval(t);
  }, []);
  const row =
    "flex h-8 w-full items-center gap-2.5 rounded-lg px-2.5 text-left text-[13px] outline-none hover:bg-hover focus-visible:ring-2 focus-visible:ring-tint";
  const heading = "flex h-7 items-center px-2.5 text-[12px] font-medium text-muted-foreground";
  // Every agent pane in every session, with what it's doing.
  const all = sessions.flatMap((g) =>
    panes(g.layout.root)
      .filter((p) => p.agentId)
      .map((p) => ({
        pane: p,
        session: g,
        st: statuses[p.id] ?? { status: "idle" as Status, since: now },
      })),
  );
  const nameOf = (p: Pane) =>
    p.title ??
    projects.find((x) => x.id === p.projectId)?.name ??
    agents.find((a) => a.id === p.agentId)?.name ??
    p.agentId;
  return (
    <aside
      aria-label="Sessions and agents"
      data-entity="sidebar"
      className="flex w-62 shrink-0 flex-col border-r border-border bg-sidebar pt-3 pb-2.5 select-none"
    >
      <section aria-label="Sessions" className="flex min-h-0 flex-1 flex-col px-2.5">
        <div className={heading}>
          <span className="flex-1">Sessions</span>
          <Hint text="New session">
            <button
              type="button"
              aria-label="New session"
              data-local="session.new"
              onClick={onAddSession}
              className="grid size-6 place-items-center rounded-md text-muted-foreground outline-none hover:bg-hover hover:text-foreground focus-visible:ring-2 focus-visible:ring-tint"
            >
              <PlusIcon className="size-4" />
            </button>
          </Hint>
        </div>
        <div className="flex min-h-0 flex-col gap-0.5 overflow-y-auto">
          {sessions.map((g) =>
            renaming === g.id ? (
              <input
                key={g.id}
                autoFocus
                defaultValue={g.name}
                aria-label="Session name"
                onFocus={(e) => e.currentTarget.select()}
                onBlur={(e) => onRenamedSession(g.id, e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") onRenamedSession(g.id, e.currentTarget.value);
                  if (e.key === "Escape") onRenamedSession(g.id, g.name);
                }}
                className="h-8 rounded-lg border border-tint-edge bg-raised px-2.5 text-[13px] outline-none"
              />
            ) : (
              <ContextMenu key={g.id}>
                <ContextMenuTrigger asChild>
                  <button
                    type="button"
                    data-entity="session"
                    data-id={g.id}
                    onClick={() => onSelectSession(g.id)}
                    onDoubleClick={() => onRenameSession(g.id)}
                    className={cn(
                      row,
                      !settings && current === g.id
                        ? "bg-hover text-foreground"
                        : "text-muted-foreground hover:text-foreground",
                    )}
                  >
                    <Hint text={g.name}>
                      <span className="truncate">{g.name}</span>
                    </Hint>
                  </button>
                </ContextMenuTrigger>
                <ContextMenuContent className="w-44 rounded-xl p-1.5">
                  <ContextMenuItem onSelect={() => onRenameSession(g.id)}>
                    <PencilIcon /> Rename
                  </ContextMenuItem>
                  <ContextMenuItem variant="destructive" onSelect={() => onRemoveSession(g.id)}>
                    <Trash2Icon /> Close session
                  </ContextMenuItem>
                </ContextMenuContent>
              </ContextMenu>
            ),
          )}
        </div>
      </section>
      <section
        aria-label="Agents"
        className="flex min-h-0 flex-1 flex-col border-t border-border px-2.5 pt-2"
      >
        <div className={heading}>
          <span className="flex-1">Agents</span>
          <Hint text="New agent (Ctrl T)">
            <button
              type="button"
              aria-label="New agent"
              data-local="pane.new"
              onClick={onNewAgent}
              className="grid size-6 place-items-center rounded-md text-muted-foreground outline-none hover:bg-hover hover:text-foreground focus-visible:ring-2 focus-visible:ring-tint"
            >
              <PlusIcon className="size-4" />
            </button>
          </Hint>
        </div>
        <div className="flex min-h-0 flex-col overflow-y-auto">
          {all.length === 0 && (
            <p className="px-2.5 py-1 text-[12px] text-muted-foreground">No agents running yet.</p>
          )}
          {ORDER.map((status) => {
            const here = all.filter((a) => a.st.status === status);
            if (!here.length) return null;
            return (
              <div key={status} className="flex flex-col gap-0.5 pb-2">
                <p className="flex items-center gap-1.5 px-2.5 pt-1 pb-0.5 text-[12px]">
                  <span className={TONE[status]}>{STATUS_LABEL[status]}</span>
                  <span className="text-muted-foreground">{here.length}</span>
                </p>
                {here.map(({ pane, session, st }) => {
                  const agent = agents.find((a) => a.id === pane.agentId);
                  const on = !settings && current === session.id && focusedPane === pane.id;
                  return (
                    <Hint
                      text={`${nameOf(pane)} · ${agent?.name ?? pane.agentId} · ${session.name}`}
                    >
                      <button
                        key={pane.id}
                        type="button"
                        data-entity="pane"
                        data-id={pane.id}
                        onClick={() => onSelectPane(pane.id)}
                        className={cn(row, on && "bg-hover")}
                      >
                        <ProviderIcon id={agent?.provider ?? pane.agentId!} size={15} />
                        <span className="min-w-0 flex-1 truncate">{nameOf(pane)}</span>
                        <span className="shrink-0 text-[11.5px] text-muted-foreground tabular-nums">
                          {ago(st.since, now)}
                        </span>
                        <StatusIcon status={st.status} />
                      </button>
                    </Hint>
                  );
                })}
              </div>
            );
          })}
        </div>
      </section>
      <div className="flex flex-col gap-0.5 border-t border-border px-2.5 pt-2">
        <AppUpdate onOpen={onUpdates} />
        <button
          type="button"
          data-command="layout.focus"
          data-arg="settings"
          onClick={onSettings}
          className={cn(
            row,
            "text-muted-foreground hover:text-foreground",
            settings && "bg-hover text-foreground",
          )}
        >
          <SettingsIcon className="size-4" /> Settings
        </button>
      </div>
    </aside>
  );
}
