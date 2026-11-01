// Shapes the Deck hands its views.
import type { Harness } from "@/lib/harnesses";

/** A command a control runs, by its registry name; the Deck routes it. */
export type RunCommand = (name: string, input: Record<string, unknown>) => void;

/** The agents, as Settings shows them. */
export type SpawnData = {
  harnesses: Harness[];
};
