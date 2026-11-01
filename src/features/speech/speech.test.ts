import { describe, expect, it } from "vitest";
import { resample, toBase64 } from "./speech";

describe("speech recording", () => {
  it("sends little-endian f32 samples, as speech.transcribe decodes them", () => {
    const pcm = new Float32Array([0, 0.5, -1]);
    const bytes = Uint8Array.from(atob(toBase64(pcm)), (c) => c.charCodeAt(0));
    const back = new DataView(bytes.buffer);
    expect([0, 4, 8].map((i) => back.getFloat32(i, true))).toEqual([0, 0.5, -1]);
  });

  it("resamples 48 kHz down to the model's 16 kHz", () => {
    const pcm = Float32Array.from({ length: 4800 }, (_, i) => i / 4800);
    const out = resample(pcm, 48_000, 16_000);
    expect(out.length).toBe(1600);
    expect(out[800]).toBeCloseTo(pcm[2400]);
  });
});
