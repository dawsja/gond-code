import { useEffect, useState } from "react";
import { SearchIcon } from "lucide-react";
import { Dialog as DialogPrimitive } from "radix-ui";
import { Kbd, KbdGroup } from "@/components/ui/kbd";

const SHOW = "gond:keys";

/** Opens the shortcut list (Ctrl+Shift+P, wherever focus is). */
export const showKeys = () => window.dispatchEvent(new Event(SHOW));

/** Every key Gond Code answers to, as a quick reference: the list only shows them. */
const GROUPS: { title: string; keys: { keys: string[]; does: string }[] }[] = [
  {
    title: "Panes",
    keys: [
      { keys: ["Ctrl", "T"], does: "New agent" },
      { keys: ["Ctrl", "Shift", "→"], does: "Split right" },
      { keys: ["Ctrl", "Shift", "↓"], does: "Split down" },
      { keys: ["Ctrl", "Shift", "Z"], does: "Zoom the pane, or unzoom it" },
      { keys: ["Ctrl", "Shift", "W"], does: "Close the pane" },
    ],
  },
  {
    title: "Terminal",
    keys: [
      { keys: ["Ctrl", "Shift", "C"], does: "Copy" },
      { keys: ["Ctrl", "Shift", "V"], does: "Paste" },
      { keys: ["Ctrl", "C"], does: "Copy the selection; with none, stop the program" },
    ],
  },
  {
    title: "Speech",
    keys: [
      { keys: ["Ctrl", "Shift", "Space"], does: "Start talking; again to type it in" },
      { keys: ["Esc"], does: "Stop talking without typing it" },
    ],
  },
  {
    title: "Help",
    keys: [{ keys: ["Ctrl", "Shift", "P"], does: "This list" }],
  },
];

/**
 * The keyboard shortcuts, a command-menu-style list.
 */
export function KeyHelp() {
  const [open, setOpen] = useState(false);
  const [q, setQ] = useState("");
  useEffect(() => {
    const show = () => {
      setQ("");
      setOpen(true);
    };
    window.addEventListener(SHOW, show);
    return () => window.removeEventListener(SHOW, show);
  }, []);

  const find = q.trim().toLowerCase();
  const groups = GROUPS.map((g) => ({
    ...g,
    keys: g.keys.filter(
      (k) => !find || `${k.does} ${k.keys.join(" ")}`.toLowerCase().includes(find),
    ),
  })).filter((g) => g.keys.length);

  return (
    <DialogPrimitive.Root open={open} onOpenChange={setOpen}>
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay className="fixed inset-0 z-50 bg-black/50" />
        <DialogPrimitive.Content
          aria-describedby={undefined}
          className="fixed top-[14vh] left-1/2 z-50 flex max-h-[70vh] w-[520px] -translate-x-1/2 flex-col overflow-hidden rounded-2xl border border-edge bg-card shadow-[0_24px_60px_rgba(0,0,0,.7)] outline-none"
        >
          <DialogPrimitive.Title className="sr-only">Keyboard shortcuts</DialogPrimitive.Title>
          <label className="flex items-center gap-2.5 border-b border-edge px-4">
            <SearchIcon className="size-4 shrink-0 text-muted-foreground" />
            <input
              autoFocus
              value={q}
              onChange={(e) => setQ(e.target.value)}
              placeholder="Search shortcuts"
              aria-label="Search shortcuts"
              className="h-12 min-w-0 flex-1 bg-transparent text-sm outline-none placeholder:text-faint"
            />
          </label>
          <div className="overflow-y-auto p-1.5">
            {groups.map((g) => (
              <section key={g.title} aria-label={g.title} className="pb-1">
                <p className="px-2.5 pt-2 pb-1 text-[11.5px] font-medium text-faint">{g.title}</p>
                {g.keys.map((k) => (
                  <div
                    key={k.does}
                    className="flex items-center gap-3 rounded-xl px-2.5 py-1.5 text-sm"
                  >
                    <span className="min-w-0 flex-1 truncate">{k.does}</span>
                    <KbdGroup>
                      {k.keys.map((key) => (
                        <Kbd key={key}>{key}</Kbd>
                      ))}
                    </KbdGroup>
                  </div>
                ))}
              </section>
            ))}
            {!groups.length && (
              <p className="px-2.5 py-6 text-center text-sm text-muted-foreground">
                No shortcut matches “{q.trim()}”.
              </p>
            )}
          </div>
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
}
