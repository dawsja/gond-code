import { useEffect, useRef, useState } from "react";
import { SearchIcon } from "lucide-react";
import { Dialog as DialogPrimitive } from "radix-ui";
import { ProviderIcon } from "@/components/brand/provider-icon";
import { Kbd, KbdGroup } from "@/components/ui/kbd";
import type { AgentInfo } from "@/lib/types";
import { cn } from "@/lib/utils";
import { panes, type Pane, type Session } from "./layout";
import { usePaneAgents } from "./paneAgent";
import { onShowSearch, type Hit } from "./scrollbackSearch";
import { handles } from "./Terminal";

/** Matches kept per pane, and in all. */
const PER_PANE = 50;
const IN_ALL = 300;

type Group = { pane: Pane; session: Session; hits: Hit[] };

/**
 * Search across every pane (Ctrl+Shift+F): what any terminal in any session printed, from the
 * newest. Choosing a match opens its session, focuses the pane, scrolls to the line and selects
 * it.
 */
export function PaneSearch({
  sessions,
  agents,
  onReveal,
}: {
  sessions: Session[];
  agents: AgentInfo[];
  /** Shows the pane: its session, focused. */
  onReveal: (paneId: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [q, setQ] = useState("");
  const [groups, setGroups] = useState<Group[]>([]);
  const [sel, setSel] = useState(0);
  const running = usePaneAgents();
  const list = useRef<HTMLDivElement>(null);

  useEffect(
    () =>
      onShowSearch(() => {
        setQ("");
        setGroups([]);
        setSel(0);
        setOpen(true);
      }),
    [],
  );

  // Every pane's scrollback, a moment after you stop typing.
  useEffect(() => {
    if (!open) return;
    const t = window.setTimeout(() => {
      let left = IN_ALL;
      const found: Group[] = [];
      for (const session of sessions)
        for (const pane of panes(session.layout.root)) {
          if (left <= 0) break;
          const hits = handles.get(pane.id)?.search(q, Math.min(PER_PANE, left)) ?? [];
          if (hits.length) found.push({ pane, session, hits });
          left -= hits.length;
        }
      setGroups(found);
      setSel(0);
    }, 120);
    return () => window.clearTimeout(t);
  }, [q, open, sessions]);

  const flat = groups.flatMap((g) => g.hits.map((hit) => ({ paneId: g.pane.id, hit })));
  const choose = (n: number) => {
    const c = flat[n];
    if (!c) return;
    setOpen(false);
    onReveal(c.paneId);
    // Once its session shows and the pane has its size.
    window.setTimeout(() => {
      const h = handles.get(c.paneId);
      h?.reveal(c.hit);
      h?.focus();
    }, 80);
  };
  useEffect(() => {
    list.current?.querySelector('[data-selected="true"]')?.scrollIntoView({ block: "nearest" });
  }, [sel]);

  const agentOf = (p: Pane) => agents.find((a) => a.id === running[p.id]);
  const nameOf = (p: Pane) => p.title ?? agentOf(p)?.name ?? running[p.id] ?? "Terminal";
  let index = -1;
  return (
    <DialogPrimitive.Root open={open} onOpenChange={setOpen}>
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay className="fixed inset-0 z-50 bg-black/50" />
        <DialogPrimitive.Content
          aria-describedby={undefined}
          onCloseAutoFocus={(e) => e.preventDefault()}
          className="fixed top-[12vh] left-1/2 z-50 flex max-h-[72vh] w-[min(720px,calc(100vw-48px))] -translate-x-1/2 flex-col overflow-hidden rounded-2xl border border-edge bg-card shadow-[0_24px_60px_rgba(0,0,0,.7)] outline-none"
        >
          <DialogPrimitive.Title className="sr-only">Search every pane</DialogPrimitive.Title>
          <label className="flex items-center gap-2.5 border-b border-edge px-4">
            <SearchIcon className="size-4 shrink-0 text-muted-foreground" />
            <input
              autoFocus
              value={q}
              onChange={(e) => setQ(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "ArrowDown") {
                  e.preventDefault();
                  setSel((s) => Math.min(s + 1, flat.length - 1));
                } else if (e.key === "ArrowUp") {
                  e.preventDefault();
                  setSel((s) => Math.max(s - 1, 0));
                } else if (e.key === "Enter") {
                  e.preventDefault();
                  choose(sel);
                }
              }}
              placeholder="Search every pane"
              aria-label="Search every pane"
              data-local="pane.search"
              className="h-12 min-w-0 flex-1 bg-transparent text-sm outline-none placeholder:text-faint"
            />
            {q.trim() && (
              <span className="shrink-0 text-[12px] text-muted-foreground tabular-nums">
                {flat.length >= IN_ALL ? `${IN_ALL}+` : flat.length}{" "}
                {flat.length === 1 ? "match" : "matches"}
              </span>
            )}
          </label>
          <div ref={list} role="listbox" aria-label="Matches" className="overflow-y-auto p-1.5">
            {groups.map((g) => (
              <section key={g.pane.id} aria-label={nameOf(g.pane)} className="pb-1">
                <p className="flex items-center gap-2 px-2.5 pt-2 pb-1 text-[11.5px] text-faint">
                  {running[g.pane.id] && (
                    <ProviderIcon id={agentOf(g.pane)?.provider ?? running[g.pane.id]} size={12} />
                  )}
                  <span className="font-medium text-muted-foreground">{nameOf(g.pane)}</span>
                  <span>·</span>
                  <span>{g.session.name}</span>
                  <span className="ml-auto tabular-nums">{g.hits.length}</span>
                </p>
                {g.hits.map((hit) => {
                  const n = ++index;
                  return (
                    <button
                      key={`${hit.row}:${hit.col}`}
                      type="button"
                      role="option"
                      aria-selected={n === sel}
                      data-selected={n === sel}
                      onMouseMove={() => setSel(n)}
                      onClick={() => choose(n)}
                      className={cn(
                        "flex w-full min-w-0 items-baseline rounded-lg px-2.5 py-1 text-left font-mono text-[12px] tracking-normal outline-none",
                        n === sel && "bg-hover",
                      )}
                    >
                      <span className="min-w-0 truncate whitespace-pre text-muted-foreground">
                        {hit.before}
                        <mark className="rounded-sm bg-tint/35 text-foreground">{hit.match}</mark>
                        {hit.after}
                      </span>
                    </button>
                  );
                })}
              </section>
            ))}
            {!q.trim() && (
              <p className="px-2.5 py-6 text-center text-sm text-muted-foreground">
                Search what every pane printed, in every session.
              </p>
            )}
            {q.trim() && !groups.length && (
              <p className="px-2.5 py-6 text-center text-sm text-muted-foreground">
                No pane printed &ldquo;{q.trim()}&rdquo;.
              </p>
            )}
          </div>
          <footer className="flex items-center gap-4 border-t border-edge px-4 py-2 text-[11.5px] text-faint">
            <span className="flex items-center gap-1.5">
              <KbdGroup>
                <Kbd>↑</Kbd>
                <Kbd>↓</Kbd>
              </KbdGroup>
              to move
            </span>
            <span className="flex items-center gap-1.5">
              <Kbd>Enter</Kbd> to go to it
            </span>
            <span className="flex items-center gap-1.5">
              <Kbd>Esc</Kbd> to close
            </span>
          </footer>
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
}
