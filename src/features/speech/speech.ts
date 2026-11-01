import { useAppState } from "@/lib/appState";

/** `/speech` in the tree (`api::speech`): whether the model is here, or coming. */
export type SpeechState =
  | { state: "off" }
  | { state: "downloading"; percent: number }
  | { state: "ready" }
  | { state: "error"; error: string }
  /** This PC's processor can't run the engine (no AVX2 and the rest): never offered. */
  | { state: "unsupported"; error: string };

export const useSpeech = () =>
  useAppState((t) => t.speech as SpeechState | undefined) ?? { state: "off" as const };

/** The model takes 16 kHz mono. */
const RATE = 16_000;

export type Recording = {
  /** Stops and hands back what was said, as 16 kHz mono samples. */
  stop: () => Float32Array;
  cancel: () => void;
  /** How loud it is right now, 0 to 1, for the bars. */
  level: () => number;
};

/** Starts recording from the microphone. Throws if there's no microphone or it's refused. */
export async function record(): Promise<Recording> {
  const stream = await navigator.mediaDevices.getUserMedia({
    audio: { channelCount: 1, echoCancellation: true, noiseSuppression: true },
  });
  const ctx = new AudioContext({ sampleRate: RATE });
  const source = ctx.createMediaStreamSource(stream);
  // ScriptProcessor rather than a worklet: a worklet needs a module URL the CSP doesn't allow.
  const node = ctx.createScriptProcessor(4096, 1, 1);
  const chunks: Float32Array[] = [];
  node.onaudioprocess = (e) => chunks.push(new Float32Array(e.inputBuffer.getChannelData(0)));
  source.connect(node);
  node.connect(ctx.destination);
  const analyser = ctx.createAnalyser();
  analyser.fftSize = 512;
  source.connect(analyser);
  const frame = new Float32Array(analyser.fftSize);
  const end = () => {
    node.disconnect();
    source.disconnect();
    stream.getTracks().forEach((t) => t.stop());
    void ctx.close();
  };
  return {
    stop: () => {
      end();
      const all = new Float32Array(chunks.reduce((n, c) => n + c.length, 0));
      let at = 0;
      for (const c of chunks) {
        all.set(c, at);
        at += c.length;
      }
      return ctx.sampleRate === RATE ? all : resample(all, ctx.sampleRate, RATE);
    },
    cancel: end,
    level: () => {
      analyser.getFloatTimeDomainData(frame);
      const rms = Math.sqrt(frame.reduce((n, x) => n + x * x, 0) / frame.length);
      return Math.min(1, rms * 8);
    },
  };
}

/** Linear resampling, for a WebView that won't record at 16 kHz itself. */
export function resample(pcm: Float32Array, from: number, to: number): Float32Array {
  const out = new Float32Array(Math.floor((pcm.length * to) / from));
  for (let i = 0; i < out.length; i++) {
    const x = (i * from) / to;
    const j = Math.floor(x);
    const k = Math.min(j + 1, pcm.length - 1);
    out[i] = pcm[j] + (pcm[k] - pcm[j]) * (x - j);
  }
  return out;
}

/** The samples as `speech.transcribe` takes them: little-endian f32, base64. */
export function toBase64(pcm: Float32Array): string {
  const bytes = new Uint8Array(pcm.buffer, pcm.byteOffset, pcm.byteLength);
  let s = "";
  for (let i = 0; i < bytes.length; i += 0x8000) {
    s += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  }
  return btoa(s);
}
