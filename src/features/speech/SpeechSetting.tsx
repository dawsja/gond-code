import { useState } from "react";
import { Kbd } from "@/components/ui/kbd";
import { Switch } from "@/components/ui/switch";
import { liveBackend, runCommand } from "@/lib/appState";
import { useSpeech } from "./speech";

/**
 * Settings → General → Speech to text: on downloads NVIDIA's Parakeet model to this PC, off
 * (or uninstalling Gond Code) deletes it.
 */
export function SpeechSetting() {
  const speech = useSpeech();
  const [problem, setProblem] = useState<string>();
  if (!liveBackend()) return null;
  const on = speech.state === "downloading" || speech.state === "ready";
  const toggle = (next: boolean) => {
    setProblem(undefined);
    runCommand(next ? "speech.enable" : "speech.disable").catch((e) => setProblem(String(e)));
  };
  // A processor without the instructions the engine needs: the switch stays off, with why.
  const unsupported = speech.state === "unsupported";
  const why = problem ?? (speech.state === "error" || unsupported ? speech.error : undefined);
  return (
    <div className="grid gap-2">
      <label className="flex items-start gap-3 text-sm">
        <Switch
          checked={on}
          disabled={unsupported}
          aria-description={unsupported ? speech.error : undefined}
          onCheckedChange={toggle}
          data-command={on ? "speech.disable" : "speech.enable"}
          aria-label="Speech to text"
        />
        <span className="grid gap-1">
          <span>
            Press <Kbd>Ctrl+Shift+Space</Kbd> to talk to the pane you&apos;re in.
          </span>
          <span className="text-xs text-muted-foreground">
            Runs on this PC. Downloads a 731 MB model.
          </span>
        </span>
      </label>
      {speech.state === "downloading" && (
        <div className="ml-11 flex max-w-sm items-center gap-3" role="status">
          <span className="block h-1.5 flex-1 overflow-hidden rounded-full bg-hover">
            <span
              className="block h-full rounded-full bg-tint transition-[width]"
              style={{ width: `${speech.percent}%` }}
            />
          </span>
          <span className="text-xs text-muted-foreground tabular-nums">
            Downloading… {speech.percent}%
          </span>
        </div>
      )}
      {speech.state === "ready" && <p className="ml-11 text-xs text-muted-foreground">Ready.</p>}
      {why && <p className="ml-11 text-xs text-tone-bad">{why}</p>}
    </div>
  );
}
