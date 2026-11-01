import { useEffect, useRef, useState } from "react";
import { LoaderIcon } from "lucide-react";
import { record, toBase64, useSpeech, type Recording } from "@/features/speech/speech";
import { runCommand } from "@/lib/appState";
import { notify } from "@/lib/notify";
import { errorText } from "@/lib/types";
import { handles } from "./Terminal";

/** The keybind's event: Ctrl+Shift+Space in a pane or anywhere in the window. */
const DICTATE = "gond:dictate";
export const dictate = () => window.dispatchEvent(new Event(DICTATE));

const MAX_SECONDS = 180;
const BARS = 14;

/**
 * Talk to the agent you're in:
 * Ctrl+Shift+Space starts listening, again stops, and what you said is typed into the focused
 * pane, transcribed on this PC, for you to send. Esc throws it away.
 */
export function Dictation({ target }: { target: () => string | undefined }) {
  const speech = useSpeech();
  const [phase, setPhase] = useState<"idle" | "recording" | "working">("idle");
  const [seconds, setSeconds] = useState(0);
  const [levels, setLevels] = useState<number[]>(() => Array(BARS).fill(0));
  const rec = useRef<Recording | null>(null);
  const into = useRef<string | undefined>(undefined);
  const live = useRef({ phase, speech, target });
  useEffect(() => {
    live.current = { phase, speech, target };
  });

  useEffect(() => {
    const stop = async () => {
      const r = rec.current;
      rec.current = null;
      if (!r) return;
      const pcm = r.stop();
      if (pcm.length < 1600) return setPhase("idle");
      setPhase("working");
      try {
        const text = await runCommand<string>("speech.transcribe", { audio: toBase64(pcm) });
        const h = into.current ? handles.get(into.current) : undefined;
        if (!text) notify.info("Didn't catch anything. Try again a little closer to the mic.");
        else if (!h) notify.info("Open a pane to type into first.");
        else {
          h.insert(text.trim());
          h.focus();
        }
      } catch (e) {
        notify.error("Couldn't transcribe", { description: errorText(e) });
      } finally {
        setPhase("idle");
      }
    };
    const toggle = async () => {
      const { phase, speech, target } = live.current;
      if (phase === "working") return;
      if (phase === "recording") return void stop();
      if (speech.state !== "ready")
        return notify.info("Turn on speech to text in Settings → General first.");
      into.current = target();
      try {
        rec.current = await record();
        setSeconds(0);
        setLevels(Array(BARS).fill(0));
        setPhase("recording");
      } catch (e) {
        notify.error("Gond Code can't use the microphone", {
          description: `${errorText(e)}. Check Windows Settings → Privacy → Microphone.`,
        });
      }
    };
    const esc = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || !rec.current) return;
      rec.current.cancel();
      rec.current = null;
      setPhase("idle");
    };
    window.addEventListener(DICTATE, toggle);
    window.addEventListener("keydown", esc, true);
    return () => {
      window.removeEventListener(DICTATE, toggle);
      window.removeEventListener("keydown", esc, true);
      rec.current?.cancel();
    };
  }, []);

  useEffect(() => {
    if (phase !== "recording") return;
    const clock = setInterval(() => setSeconds((s) => s + 1), 1000);
    const bars = setInterval(
      () => setLevels((v) => [...v.slice(1), rec.current?.level() ?? 0]),
      70,
    );
    return () => {
      clearInterval(clock);
      clearInterval(bars);
    };
  }, [phase]);
  useEffect(() => {
    if (phase === "recording" && seconds >= MAX_SECONDS) dictate();
  }, [phase, seconds]);

  if (phase === "idle") return null;
  return (
    <div
      role="status"
      data-entity="dictation"
      className="pointer-events-none absolute bottom-5 left-1/2 z-30 flex h-10 -translate-x-1/2 items-center gap-2.5 rounded-full border border-tint-edge bg-tint-wash px-4 shadow-[0_12px_32px_rgba(0,0,0,.6)]"
    >
      {phase === "recording" ? (
        <>
          <span className="size-2 animate-pulse rounded-full bg-tint" />
          <span className="text-[12.5px] tabular-nums">
            {Math.floor(seconds / 60)}:{String(seconds % 60).padStart(2, "0")}
          </span>
          <span aria-hidden className="flex h-4 items-center gap-[2px]">
            {levels.map((l, i) => (
              <span
                key={i}
                className="w-[2.5px] rounded-full bg-tint transition-[height] duration-75"
                style={{ height: `${Math.max(3, Math.round(3 + l * 13))}px` }}
              />
            ))}
          </span>
          <span className="text-[11.5px] text-muted-foreground">
            Ctrl ⇧ Space to stop · Esc to cancel
          </span>
        </>
      ) : (
        <>
          <LoaderIcon className="size-3.5 animate-spin text-tint-ink" />
          <span className="text-[12.5px]">Transcribing…</span>
        </>
      )}
    </div>
  );
}
