import { useEffect, useRef, useState } from "react";
import { CornerDownLeftIcon, SearchIcon } from "lucide-react";
import { Dialog as DialogPrimitive } from "radix-ui";
import { Kbd, KbdGroup } from "@/components/ui/kbd";
import { cn } from "@/lib/utils";

const SHOW = "gond:keys";

/** Opens the command list (Ctrl+Shift+P, wherever focus is). */
export const showKeys = () => window.dispatchEvent(new Event(SHOW));

/** What a row in the list does when you run it; rows without one only explain a key. */
export type Command =
  | "new"
  | "right"
  | "down"
  | "zoom"
  | "close"
  | "rename"
  | "restart"
  | "copy"
  | "paste"
  | "search"
  | "dictate"
  | "session"
  | "settings"
  | "updates";

type Row = { keys?: string[]; does: string; run?: Command };

/** Every command and key Gond Code answers to. Enter or a click runs a command. */
const GROUPS: { title: string; rows: Row[] }[] = [
  {
    title: "Panes",
    rows: [
      { keys: ["Ctrl", "T"], does: "New terminal beside the pane", run: "new" },
      { keys: ["Ctrl", "Shift", "→"], does: "Split right: a new terminal", run: "right" },
      { keys: ["Ctrl", "Shift", "↓"], does: "Split down: a new terminal", run: "down" },
      { keys: ["Ctrl", "Shift", "Z"], does: "Zoom the pane, or unzoom it", run: "zoom" },
      { keys: ["Ctrl", "Shift", "W"], does: "Close the pane", run: "close" },
      { does: "Rename the pane", run: "rename" },
      { does: "Restart the pane's terminal", run: "restart" },
    ],
  },
  {
    title: "Terminal",
    rows: [
      { keys: ["Ctrl", "Shift", "C"], does: "Copy", run: "copy" },
      { keys: ["Ctrl", "Shift", "V"], does: "Paste", run: "paste" },
      { keys: ["Ctrl", "C"], does: "Copy the selection; with none, stop the program" },
      { keys: ["Ctrl", "Shift", "F"], does: "Search what every pane printed", run: "search" },
      { keys: ["Click"], does: "Open a file:line link in your editor" },
    ],
  },
  {
    title: "Speech",
    rows: [
      {
        keys: ["Ctrl", "Shift", "Space"],
        does: "Start talking; again to type it in",
        run: "dictate",
      },
      { keys: ["Esc"], does: "Stop talking without typing it" },
    ],
  },
  {
    title: "Gond Code",
    rows: [
      { does: "New session", run: "session" },
      { does: "Settings", run: "settings" },
      { does: "Check for updates", run: "updates" },
      { keys: ["Ctrl", "Shift", "P"], does: "This list" },
    ],
  },
];

/**
 * The commands and keys, a command-menu-style list: type to find one, ↑↓ and Enter (or a
 * click) to run it on the focused pane.
 */
export function KeyHelp({ run }: { run?: (c: Command) => void }) {
  const [open, setOpen] = useState(false);
  const [q, setQ] = useState("");
  const [sel, setSel] = useState(0);
  const list = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const show = () => {
      setQ("");
      setSel(0);
      setOpen(true);
    };
    window.addEventListener(SHOW, show);
    return () => window.removeEventListener(SHOW, show);
  }, []);

  const find = q.trim().toLowerCase();
  const groups = GROUPS.map((g) => ({
    ...g,
    rows: g.rows.filter(
      (r) => !find || `${r.does} ${(r.keys ?? []).join(" ")}`.toLowerCase().includes(find),
    ),
  })).filter((g) => g.rows.length);
  // The rows that do something, in order: ↑↓ move among them.
  const runnable = run ? groups.flatMap((g) => g.rows.filter((r) => r.run)) : [];
  const chosen = runnable[Math.min(sel, runnable.length - 1)];

  const go = (r?: Row) => {
    if (!r?.run || !run) return;
    setOpen(false);
    // Once the list has closed and given focus back to the pane.
    const c = r.run;
    window.setTimeout(() => run(c), 0);
  };
  useEffect(() => {
    list.current?.querySelector('[data-selected="true"]')?.scrollIntoView({ block: "nearest" });
  }, [sel, q]);

  return (
    <DialogPrimitive.Root open={open} onOpenChange={setOpen}>
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay className="fixed inset-0 z-50 bg-black/50" />
        <DialogPrimitive.Content
          aria-describedby={undefined}
          className="fixed top-[14vh] left-1/2 z-50 flex max-h-[70vh] w-[520px] -translate-x-1/2 flex-col overflow-hidden rounded-2xl border border-edge bg-card shadow-[0_24px_60px_rgba(0,0,0,.7)] outline-none"
        >
          <DialogPrimitive.Title className="sr-only">Commands</DialogPrimitive.Title>
          <label className="flex items-center gap-2.5 border-b border-edge px-4">
            <SearchIcon className="size-4 shrink-0 text-muted-foreground" />
            <input
              autoFocus
              value={q}
              onChange={(e) => {
                setQ(e.target.value);
                setSel(0);
              }}
              onKeyDown={(e) => {
                if (e.key === "ArrowDown") {
                  e.preventDefault();
                  setSel((s) => Math.min(s + 1, runnable.length - 1));
                } else if (e.key === "ArrowUp") {
                  e.preventDefault();
                  setSel((s) => Math.max(s - 1, 0));
                } else if (e.key === "Enter") {
                  e.preventDefault();
                  go(chosen);
                }
              }}
              placeholder="Search commands"
              aria-label="Search commands"
              data-local="keys.search"
              className="h-12 min-w-0 flex-1 bg-transparent text-sm outline-none placeholder:text-faint"
            />
          </label>
          <div ref={list} role="listbox" aria-label="Commands" className="overflow-y-auto p-1.5">
            {groups.map((g) => (
              <section key={g.title} aria-label={g.title} className="pb-1">
                <p className="px-2.5 pt-2 pb-1 text-[11.5px] font-medium text-faint">{g.title}</p>
                {g.rows.map((r) => {
                  const on = !!chosen && r === chosen;
                  const keys = r.keys && (
                    <KbdGroup>
                      {r.keys.map((key) => (
                        <Kbd key={key}>{key}</Kbd>
                      ))}
                    </KbdGroup>
                  );
                  return r.run && run ? (
                    <button
                      key={r.does}
                      type="button"
                      role="option"
                      aria-selected={on}
                      data-selected={on}
                      data-local="keys.run"
                      data-arg={r.run}
                      onMouseMove={() => setSel(runnable.indexOf(r))}
                      onClick={() => go(r)}
                      className={cn(
                        "flex w-full items-center gap-3 rounded-xl px-2.5 py-1.5 text-left text-sm outline-none",
                        on && "bg-hover",
                      )}
                    >
                      <span className="min-w-0 flex-1 truncate">{r.does}</span>
                      {on && !keys && (
                        <CornerDownLeftIcon className="size-3.5 shrink-0 text-muted-foreground" />
                      )}
                      {keys}
                    </button>
                  ) : (
                    <div
                      key={r.does}
                      className="flex items-center gap-3 rounded-xl px-2.5 py-1.5 text-sm text-muted-foreground"
                    >
                      <span className="min-w-0 flex-1 truncate">{r.does}</span>
                      {keys}
                    </div>
                  );
                })}
              </section>
            ))}
            {!groups.length && (
              <p className="px-2.5 py-6 text-center text-sm text-muted-foreground">
                Nothing matches &ldquo;{q.trim()}&rdquo;.
              </p>
            )}
          </div>
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
}
