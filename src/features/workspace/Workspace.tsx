import { useRef, type ReactNode } from "react";
import {
  ClipboardPasteIcon,
  CopyIcon,
  FolderOpenIcon,
  Maximize2Icon,
  Minimize2Icon,
  PencilIcon,
  RotateCwIcon,
  SquareSplitHorizontalIcon,
  SquareSplitVerticalIcon,
  SquareTerminalIcon,
  TextSelectIcon,
  XIcon,
} from "lucide-react";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuShortcut,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { ProviderIcon } from "@/components/brand/provider-icon";
import type { AgentInfo } from "@/lib/types";
import { cn } from "@/lib/utils";
import { panes, type Node, type Pane, type Layout } from "./layout";
import { usePaneAgents } from "./paneAgent";
import { STATUS_LABEL, type PaneStatus, type Status } from "./status";
import { handles, Terminal, type Shortcut } from "./Terminal";
import { Hint } from "@/components/hint";

type Rect = { x: number; y: number; w: number; h: number };
type Divider = { splitId: string; dir: "right" | "down"; area: Rect; at: number };

/**
 * Where everything goes, in fractions of the folder: each pane's box and each split's divider.
 * Panes are laid out flat (never nested), so a split never remounts a terminal and its agent.
 */
function measure(node: Node | null, r: Rect = { x: 0, y: 0, w: 1, h: 1 }) {
  const boxes = new Map<string, Rect>();
  const dividers: Divider[] = [];
  const walk = (n: Node, r: Rect) => {
    if (n.kind === "pane") return void boxes.set(n.id, r);
    if (n.dir === "right") {
      const w = r.w * n.ratio;
      walk(n.a, { ...r, w });
      walk(n.b, { ...r, x: r.x + w, w: r.w - w });
      dividers.push({ splitId: n.id, dir: n.dir, area: r, at: r.x + w });
    } else {
      const h = r.h * n.ratio;
      walk(n.a, { ...r, h });
      walk(n.b, { ...r, y: r.y + h, h: r.h - h });
      dividers.push({ splitId: n.id, dir: n.dir, area: r, at: r.y + h });
    }
  };
  if (node) walk(node, r);
  return { boxes, dividers };
}

const pct = (n: number) => `${n * 100}%`;

const STATUS_DOT: Record<Status, string> = {
  working: "bg-tone-info shadow-[0_0_0_3px_rgba(144,202,249,.15)]",
  needs: "bg-tone-warn",
  done: "bg-tone-ok",
  idle: "bg-faint",
  exited: "bg-tone-bad",
};

function StatusDot({ status }: { status?: Status }) {
  return <span className={cn("size-[7px] shrink-0 rounded-full", STATUS_DOT[status ?? "idle"])} />;
}

export type PaneActions = {
  focus: (paneId: string) => void;
  shortcut: (paneId: string, s: Shortcut) => void;
  rename: (paneId: string) => void;
  restart: (paneId: string) => void;
  openFolder: (projectId: string) => void;
  resize: (splitId: string, ratio: number) => void;
};

/** One folder's panes: split right and down, drag a divider to resize, right-click for the rest. */
export function Workspace({
  layout,
  visible,
  agents,
  statuses,
  restarts,
  start,
  actions,
  renaming,
  onRenamed,
  empty,
}: {
  layout: Layout;
  visible: boolean;
  agents: AgentInfo[];
  statuses: Record<string, PaneStatus>;
  /** How often each pane was restarted: a new count starts its process again. */
  restarts: Record<string, number>;
  /** Starts a pane's shell. */
  start: (pane: Pane) => (size: { cols: number; rows: number }) => Promise<{ ptyId: string }>;
  actions: PaneActions;
  /** The pane being renamed, and what to do with the new name. */
  renaming?: string;
  onRenamed: (paneId: string, title: string) => void;
  empty: ReactNode;
}) {
  const box = useRef<HTMLDivElement>(null);
  const { boxes, dividers } = measure(layout.root);
  const list = panes(layout.root);
  const zoomed = layout.zoom && boxes.has(layout.zoom) ? layout.zoom : undefined;
  const drag = (d: Divider) => (e: React.PointerEvent) => {
    e.preventDefault();
    const el = box.current!;
    const move = (ev: PointerEvent) => {
      const r = el.getBoundingClientRect();
      const f =
        d.dir === "right"
          ? ((ev.clientX - r.left) / r.width - d.area.x) / d.area.w
          : ((ev.clientY - r.top) / r.height - d.area.y) / d.area.h;
      actions.resize(d.splitId, f);
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };
  // Each pane is its own card, 3px in from its box: 6px between panes and 6px from the window's
  // edge, so every border always shows.
  return (
    <div
      data-entity="workspace"
      className={cn("relative min-h-0 min-w-0 flex-1 overflow-hidden", !visible && "hidden")}
    >
      {list.length === 0 && empty}
      <div ref={box} className="absolute inset-[3px]">
        {list.map((p) => {
          const r = zoomed === p.id ? { x: 0, y: 0, w: 1, h: 1 } : boxes.get(p.id)!;
          return (
            <div
              key={p.id}
              className={cn(
                "absolute flex p-[3px]",
                zoomed && zoomed !== p.id && "invisible",
                zoomed === p.id && "z-10",
              )}
              style={{ left: pct(r.x), top: pct(r.y), width: pct(r.w), height: pct(r.h) }}
            >
              <PaneView
                pane={p}
                agents={agents}
                status={statuses[p.id]?.status}
                focused={layout.focus === p.id}
                zoomed={zoomed === p.id}
                restart={restarts[p.id] ?? 0}
                open={start(p)}
                actions={actions}
                visible={visible}
                renaming={renaming === p.id}
                onRenamed={(title) => onRenamed(p.id, title)}
              />
            </div>
          );
        })}
        {!zoomed &&
          dividers.map((d) => (
            <div
              key={d.splitId}
              role="separator"
              aria-orientation={d.dir === "right" ? "vertical" : "horizontal"}
              onPointerDown={drag(d)}
              // A double-click puts it back where a new split starts: half and half.
              onDoubleClick={() => actions.resize(d.splitId, 0.5)}
              className={cn(
                "absolute z-20 rounded-full hover:bg-tint/40",
                d.dir === "right"
                  ? "w-[7px] -translate-x-[3px] cursor-col-resize"
                  : "h-[7px] -translate-y-[3px] cursor-row-resize",
              )}
              style={
                d.dir === "right"
                  ? { left: pct(d.at), top: pct(d.area.y), height: pct(d.area.h) }
                  : { top: pct(d.at), left: pct(d.area.x), width: pct(d.area.w) }
              }
            />
          ))}
      </div>
    </div>
  );
}

function PaneView({
  pane,
  agents,
  status,
  focused,
  zoomed,
  restart,
  open,
  actions,
  visible,
  renaming,
  onRenamed,
}: {
  pane: Pane;
  agents: AgentInfo[];
  status?: Status;
  focused: boolean;
  zoomed: boolean;
  restart: number;
  open: (size: { cols: number; rows: number }) => Promise<{ ptyId: string }>;
  actions: PaneActions;
  visible: boolean;
  renaming: boolean;
  onRenamed: (title: string) => void;
}) {
  // The agent you started in its shell, while it runs.
  const running = usePaneAgents()[pane.id];
  const agent = agents.find((a) => a.id === running);
  const name = pane.title ?? agent?.name ?? running ?? "Terminal";
  const h = () => handles.get(pane.id);
  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <section
          aria-label={name}
          data-entity="pane"
          data-id={pane.id}
          className={cn(
            "flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-[10px] border border-edge bg-black",
            focused && "border-tint-edge",
          )}
        >
          <header
            onMouseDown={() => actions.focus(pane.id)}
            className={cn(
              "flex h-8 shrink-0 items-center gap-2 border-b border-border px-2.5 text-[12px] select-none",
              focused ? "bg-tint-wash/40" : "bg-[#070707]",
            )}
          >
            {running ? (
              <ProviderIcon id={agent?.provider ?? running} size={13} />
            ) : (
              <SquareTerminalIcon className="size-3.5 text-muted-foreground" />
            )}
            {renaming ? (
              <input
                autoFocus
                defaultValue={name}
                aria-label="Pane name"
                onBlur={(e) => onRenamed(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") onRenamed(e.currentTarget.value);
                  if (e.key === "Escape") onRenamed(name);
                }}
                className="h-6 w-40 rounded-md border border-tint-edge bg-raised px-1.5 text-[12px] outline-none"
              />
            ) : (
              <span className="shrink-0 font-medium">{name}</span>
            )}
            <span className="flex-1" />
            {running && status && (
              <span
                className={cn(
                  "flex shrink-0 items-center gap-1.5 text-[11.5px] text-muted-foreground",
                  status === "needs" && "text-tone-warn",
                )}
              >
                <StatusDot status={status} />
                {STATUS_LABEL[status]}
              </span>
            )}
            <Hint text="Close pane (Ctrl Shift W)">
              <button
                type="button"
                aria-label={`Close ${name}`}
                data-local="pane.close"
                data-arg={pane.id}
                onClick={() => actions.shortcut(pane.id, "close")}
                className="grid size-5 place-items-center rounded text-faint outline-none hover:bg-hover hover:text-foreground focus-visible:ring-2 focus-visible:ring-tint"
              >
                <XIcon className="size-3.5" />
              </button>
            </Hint>
          </header>
          <Terminal
            key={`${pane.id}:${restart}`}
            paneId={pane.id}
            open={open}
            active={focused && visible}
            onFocus={() => actions.focus(pane.id)}
            onShortcut={(s) => actions.shortcut(pane.id, s)}
          />
        </section>
      </ContextMenuTrigger>
      <ContextMenuContent className="w-60 rounded-xl p-1.5">
        <ContextMenuItem onSelect={() => h()?.copy()}>
          <CopyIcon /> Copy <ContextMenuShortcut>Ctrl ⇧ C</ContextMenuShortcut>
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => h()?.paste()}>
          <ClipboardPasteIcon /> Paste <ContextMenuShortcut>Ctrl ⇧ V</ContextMenuShortcut>
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => h()?.selectAll()}>
          <TextSelectIcon /> Select all
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem onSelect={() => actions.shortcut(pane.id, "right")}>
          <SquareSplitHorizontalIcon /> Split right{" "}
          <ContextMenuShortcut>Ctrl ⇧ →</ContextMenuShortcut>
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => actions.shortcut(pane.id, "down")}>
          <SquareSplitVerticalIcon /> Split down <ContextMenuShortcut>Ctrl ⇧ ↓</ContextMenuShortcut>
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem onSelect={() => actions.rename(pane.id)}>
          <PencilIcon /> Rename
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => actions.restart(pane.id)}>
          <RotateCwIcon /> Restart terminal
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => actions.openFolder(pane.projectId)}>
          <FolderOpenIcon /> Open folder
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => actions.shortcut(pane.id, "zoom")}>
          {zoomed ? <Minimize2Icon /> : <Maximize2Icon />} {zoomed ? "Unzoom pane" : "Zoom pane"}
          <ContextMenuShortcut>Ctrl ⇧ Z</ContextMenuShortcut>
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem variant="destructive" onSelect={() => actions.shortcut(pane.id, "close")}>
          <XIcon /> Close pane <ContextMenuShortcut>Ctrl ⇧ W</ContextMenuShortcut>
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  );
}
