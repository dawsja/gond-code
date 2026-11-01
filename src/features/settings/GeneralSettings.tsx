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
import { TINTS } from "@/lib/theme";
import { SpeechSetting } from "@/features/speech/SpeechSetting";
import { cn } from "@/lib/utils";
import type { Settings, Tint } from "@/lib/types";

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

/** Settings → General: the tint, the default shell, speech to text and closing the window. */
export function GeneralSettings({ settings, onChange, version }: Props) {
  return (
    <section aria-label="General" className="flex max-w-xl flex-col gap-5">
      <header>
        <h2 className="text-lg font-semibold">General</h2>
        <p className="text-sm text-muted-foreground">Saved on this computer.</p>
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
        <p className="text-xs text-muted-foreground">Used for new terminals.</p>
      </Field>
      <Field label="Speech to text">
        <SpeechSetting />
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
      {version && <p className="text-xs text-muted-foreground">Gond Code {version}</p>}
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
