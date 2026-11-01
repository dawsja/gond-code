import { afterEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
let inTauri = true;
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...a: unknown[]) => invoke(...a),
  isTauri: () => inTauri,
}));
afterEach(() => {
  inTauri = true;
});

describe("ipc", () => {
  it("app_version runs app.version through the registry (command_run)", async () => {
    invoke.mockResolvedValueOnce("0.1.0");
    const { call } = await import("./ipc");
    await expect(call("app_version", {})).resolves.toBe("0.1.0");
    expect(invoke).toHaveBeenCalledWith("command_run", { name: "app.version", input: {} });
  });

  it("falls back to the mock outside Tauri", async () => {
    inTauri = false;
    const { call } = await import("./ipc");
    const projects = await call("project_list", {});
    expect(projects.length).toBeGreaterThan(0);
    const added = await call("project_add", { path: "C:/src/new-one" });
    expect(added.name).toBe("new-one");
    const listed = await call("project_list", {});
    expect(listed[0].id).toBe(added.id);
  });
});
