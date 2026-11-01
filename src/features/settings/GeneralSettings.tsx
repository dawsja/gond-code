import { useEffect, useState } from "react";
import { CheckIcon, ChevronDownIcon } from "lucide-react";
import { shellBrand } from "@/components/brand/brands";
import { ProviderIcon } from "@/components/brand/provider-icon";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { isRegistered, liveBackend, runCommand } from "@/lib/appState";
import { Switch } from "@/components/ui/switch";
import { useAppName } from "@/lib/appName";
import { TINTS } from "@/lib/theme";
import { fontStack, TERMINAL_FONTS } from "@/lib/terminalFont";
import { SpeechSetting } from "@/features/speech/SpeechSetting";
import { cn } from "@/lib/utils";
import type { Settings, TerminalFont, Tint } from "@/lib/types";

// Each tint's own colour, for its swatch (the page's --tint only knows the chosen one).
const SWATCH: Record<Tint, string> = {
  blue: "#2F6FD6",
  purple: "#7650D8",
  pink: "#CF3D85",
  red: "#D03A3A",
  orange: "#E04E0A",
  amber: "#D98A08",
  green: "#189C47",
};

type Props = {
  settings: Settings;
  onChange: (patch: Partial<Settings>) => void;
  version?: string;
  /** Installed harnesses. */
  agents: { id: string; label: string; brand: string }[];
};

/** The shells on this PC (shell.list): PowerShell, Command Prompt, Git Bash, WSL distributions. */
function ShellPicker({
  value,
  onChange,
}: {
  value?: string | null;
  onChange: (id: string | null) => void;
}) {
  const [shells, setShells] = useState<{ id: string; label: string }[]>();
  useEffect(() => {
    if (!liveBackend() || !isRegistered("shell.list")) return;
    runCommand<{ id: string; label: string }[]>("shell.list", {}).then(setShells, () =>
      setShells([]),
    );
  }, []);
  const options = [
    { id: "", label: "Default" },
    ...(shells ?? []),
    ...(value && shells && !shells.some((s) => s.id === value)
      ? [{ id: value, label: `${value} (not found)` }]
      : []),
  ];
  const on = options.find((o) => o.id === (value ?? "")) ?? options[0];
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        aria-label={`Default shell: ${on.label}`}
        data-command="settings.set"
        data-arg="shell"
        className="flex h-9 w-full max-w-72 items-center gap-2 rounded-[10px] border border-edge bg-raised px-2.5 text-left text-sm outline-none hover:bg-hover focus-visible:ring-2 focus-visible:ring-tint data-[state=open]:border-tint-edge"
      >
        <ProviderIcon id={shellBrand(on.id)} />
        <span className="min-w-0 flex-1 truncate">{on.label}</span>
        <ChevronDownIcon className="size-3.5 shrink-0 text-muted-foreground" />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-72 rounded-2xl p-1.5 flex flex-col gap-0.5">
        {options.map((o) => (
          <DropdownMenuItem
            key={o.id || "default"}
            // Default is null: an undefined patch key never reaches the backend, so it never
            // cleared the pick.
            onSelect={() => onChange(o.id || null)}
            className={cn("gap-2.5 rounded-xl px-2 py-1.5", o.id === on.id && "bg-tint-wash")}
          >
            <ProviderIcon id={shellBrand(o.id)} />
            <span className="min-w-0 flex-1 truncate font-medium">{o.label}</span>
            {o.id === on.id && <CheckIcon className="size-3.5 shrink-0 text-tint-ink" />}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/** Where a `file:line` link in a pane opens: an editor on this PC (file.editors), or the file's
 *  default app. None picked = the first editor found. */
function EditorPicker({
  value,
  onChange,
}: {
  value?: string | null;
  onChange: (id: string) => void;
}) {
  const [editors, setEditors] = useState<{ id: string; label: string }[]>();
  useEffect(() => {
    if (!liveBackend() || !isRegistered("file.editors")) return;
    runCommand<{ id: string; label: string }[]>("file.editors", {}).then(setEditors, () =>
      setEditors([]),
    );
  }, []);
  const options = [
    ...(editors ?? []),
    ...(value && value !== "system" && editors && !editors.some((e) => e.id === value)
      ? [{ id: value, label: `${value} (not found)` }]
      : []),
    { id: "system", label: "Default app" },
  ];
  const on =
    options.find((o) => o.id === (value ?? editors?.[0]?.id ?? "system")) ?? options.at(-1)!;
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        aria-label={`Open files in: ${on.label}`}
        data-command="settings.set"
        data-arg="editor"
        className="flex h-9 w-full max-w-72 items-center gap-2 rounded-[10px] border border-edge bg-raised px-2.5 text-left text-sm outline-none hover:bg-hover focus-visible:ring-2 focus-visible:ring-tint data-[state=open]:border-tint-edge"
      >
        <span className="min-w-0 flex-1 truncate">{on.label}</span>
        <ChevronDownIcon className="size-3.5 shrink-0 text-muted-foreground" />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-72 rounded-2xl p-1.5 flex flex-col gap-0.5">
        {options.map((o) => (
          <DropdownMenuItem
            key={o.id}
            onSelect={() => onChange(o.id)}
            className={cn("gap-2.5 rounded-xl px-2 py-1.5", o.id === on.id && "bg-tint-wash")}
          >
            <span className="min-w-0 flex-1 truncate font-medium">{o.label}</span>
            {o.id === on.id && <CheckIcon className="size-3.5 shrink-0 text-tint-ink" />}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/** The terminals' font: one of the three Nerd Fonts the app ships, each shown in itself. */
function FontPicker({
  value,
  onChange,
}: {
  value?: TerminalFont;
  onChange: (font: TerminalFont) => void;
}) {
  const on = TERMINAL_FONTS.find((f) => f.id === value) ?? TERMINAL_FONTS[0];
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        aria-label={`Terminal font: ${on.label}`}
        data-command="settings.set"
        data-arg="terminalFont"
        className="flex h-9 w-full max-w-72 items-center gap-2 rounded-[10px] border border-edge bg-raised px-2.5 text-left text-sm outline-none hover:bg-hover focus-visible:ring-2 focus-visible:ring-tint data-[state=open]:border-tint-edge"
      >
        <span className="min-w-0 flex-1 truncate" style={{ fontFamily: fontStack(on.id) }}>
          {on.label}
        </span>
        <ChevronDownIcon className="size-3.5 shrink-0 text-muted-foreground" />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-72 rounded-2xl p-1.5 flex flex-col gap-0.5">
        {TERMINAL_FONTS.map((f) => (
          <DropdownMenuItem
            key={f.id}
            onSelect={() => onChange(f.id)}
            className={cn("gap-2.5 rounded-xl px-2 py-1.5", f.id === on.id && "bg-tint-wash")}
          >
            <span className="min-w-0 flex-1 truncate" style={{ fontFamily: fontStack(f.id) }}>
              {f.label}
            </span>
            {f.id === on.id && <CheckIcon className="size-3.5 shrink-0 text-tint-ink" />}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/** Settings → General: the tint, the default shell, the terminal font, speech to text and
 *  closing the window. */
export function GeneralSettings({ settings, onChange, version }: Props) {
  const name = useAppName();
  return (
    <section aria-label="General" className="flex max-w-xl flex-col gap-5">
      <header>
        <h2 className="text-lg font-semibold">General</h2>
        <p className="text-sm text-muted-foreground">
          How Gond Code looks and works: its colour, the shell new terminals open in and their font,
          transcription and what closing the window does.
        </p>
      </header>
      <Field label="Colour">
        <div role="radiogroup" aria-label="Colour" className="flex gap-2">
          {TINTS.map((t) => (
            <button
              key={t}
              role="radio"
              aria-checked={settings.tint === t}
              aria-label={t}
              onClick={() => onChange({ tint: t })}
              style={{ background: SWATCH[t] }}
              className="grid size-7 place-items-center rounded-full text-primary-foreground outline-none focus-visible:ring-3 focus-visible:ring-ring/30"
            >
              {settings.tint === t && <CheckIcon className="size-4" />}
            </button>
          ))}
        </div>
      </Field>
      <Field label="Default shell">
        <ShellPicker value={settings.shell} onChange={(shell) => onChange({ shell })} />
      </Field>
      <Field label="Open files in">
        <EditorPicker value={settings.editor} onChange={(editor) => onChange({ editor })} />
      </Field>
      <Field label="Terminal font">
        <FontPicker
          value={settings.terminalFont}
          onChange={(terminalFont) => onChange({ terminalFont })}
        />
      </Field>
      <Field label="Transcription">
        <SpeechSetting />
      </Field>
      <Field label="Sounds">
        <label className="flex items-start gap-3 text-sm">
          <Switch
            checked={settings.sounds !== false}
            onCheckedChange={(on) => onChange({ sounds: on })}
            data-command="settings.set"
            data-arg="sounds"
            aria-label="Play sounds"
          />
          <span className="grid gap-1">
            <span>Play a sound when an agent is done or needs you</span>
            <span className="text-xs text-muted-foreground">
              A chime when one finishes while you&apos;re elsewhere, an alert when one asks you
              something. Several at once play one after another.
            </span>
          </span>
        </label>
      </Field>
      <Field label="Quitting">
        <label className="flex items-start gap-3 text-sm">
          <Switch
            checked={settings.keepTerminals !== false}
            onCheckedChange={(on) => onChange({ keepTerminals: on })}
            data-command="settings.set"
            data-arg="keepTerminals"
            aria-label="Keep terminals running"
          />
          <span className="grid gap-1">
            <span>Keep terminals running</span>
            <span className="text-xs text-muted-foreground">
              Agents keep working when Gond Code quits, updates or crashes, and are back in their
              panes when it opens. The tray&apos;s Quit and close all terminals stops them.
            </span>
          </span>
        </label>
      </Field>
      <Field label="Closing the window">
        <label className="flex items-start gap-3 text-sm">
          <Switch
            checked={settings.closeToTray !== false}
            onCheckedChange={(on) => onChange({ closeToTray: on })}
            data-command="settings.set"
            data-arg="closeToTray"
            aria-label="Keep running in the tray"
          />
          <span className="grid gap-1">
            <span>Keep running in the tray</span>
            <span className="text-xs text-muted-foreground">
              Agents keep working after you close the window.
            </span>
          </span>
        </label>
      </Field>
      {version && (
        <p className="text-xs text-muted-foreground">
          {name} {version}
        </p>
      )}
    </section>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="grid gap-2">
      <span className="text-xs font-medium text-muted-foreground">{label}</span>
      {children}
    </div>
  );
}
