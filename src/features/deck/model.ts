// Shapes the Deck hands its views.
import type { Account, Harness, ModelInfo } from "@/lib/harnesses";

/** A command a control runs, by its registry name; the Deck routes it. */
export type RunCommand = (name: string, input: Record<string, unknown>) => void;

/** The agents, their models and the accounts they sign in with, as Settings and first run show them. */
export type SpawnData = {
  harnesses: Harness[];
  models: Record<string, ModelInfo[]>;
  accounts: Account[];
  /** model.list { harness, accountId }: an endpoint account's own models. Live only. */
  accountModels?: (harness: string, accountId: string) => Promise<ModelInfo[]>;
};
