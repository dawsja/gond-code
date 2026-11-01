import { describe, expect, it } from "vitest";
import generated from "./commands.generated.json";
import { REGISTRY_NAME } from "./ipc";

const registered = new Set(generated.map((c: { name: string }) => c.name));

describe("ipc → registry names", () => {
  it("maps only to registered AGENT-API names: every old IPC has moved", () => {
    const pending = Object.values(REGISTRY_NAME)
      .filter((n): n is string => !!n && !registered.has(n))
      .sort();
    // Every mapped name is registered now, so call sends them all through command_run.
    // A new mapping must name a registered command.
    expect(pending).toEqual([]);
  });
});
