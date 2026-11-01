// @vitest-environment happy-dom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { check } from "../../../e2e/legibility/legibility";
import { UninstallDialog, type UninstallPlan } from "./UninstallDialog";

vi.mock("@/lib/appState", () => ({ isRegistered: () => true, runCommand: vi.fn() }));
afterEach(cleanup);

const agy = { id: "agy", label: "Antigravity", brand: "antigravity" };
const plan = (removeEverything: boolean, running = 0): UninstallPlan => ({
  harnessId: "agy",
  methods: ["vendor"],
  deletes: removeEverything
    ? ["C:\\Users\\d\\AppData\\Local\\agy", "C:\\Users\\d\\.gemini\\antigravity-cli"]
    : ["C:\\Users\\d\\AppData\\Local\\agy"],
  pathEntries: ["C:\\Users\\d\\AppData\\Local\\agy\\bin"],
  keepData: !removeEverything,
  running,
});

it("lists exactly what goes, keeps settings by default, and re-reads the plan on Remove everything", async () => {
  const load = vi.fn(async (_id: string, all: boolean) => plan(all));
  const onUninstall = vi.fn();
  render(<UninstallDialog harness={agy} load={load} onUninstall={onUninstall} onClose={vi.fn()} />);
  await act(async () => {});
  expect(load).toHaveBeenLastCalledWith("agy", false);
  expect(screen.getByRole("list", { name: "Deletes" }).textContent).toBe(
    "C:\\Users\\d\\AppData\\Local\\agy",
  );
  expect(screen.getByRole("list", { name: "Takes off your PATH" }).textContent).toContain(
    "agy\\bin",
  );
  expect(check(document.body)).toEqual([]);

  await act(async () => fireEvent.click(screen.getByRole("radio", { name: "Remove everything" })));
  expect(load).toHaveBeenLastCalledWith("agy", true);
  expect(screen.getByRole("list", { name: "Deletes" }).children).toHaveLength(2);
  fireEvent.click(screen.getByRole("button", { name: "Remove everything" }));
  expect(onUninstall).toHaveBeenCalledWith(true);
});

it("refuses while that harness's agents run", async () => {
  render(
    <UninstallDialog
      harness={agy}
      load={async () => plan(false, 2)}
      onUninstall={vi.fn()}
      onClose={vi.fn()}
    />,
  );
  await act(async () => {});
  expect(screen.getByRole("alert").textContent).toBe("Stop its 2 running agents first");
  expect(screen.getByRole("button", { name: "Uninstall" })).toHaveProperty("disabled", true);
});

it("a plan that can't be read is one plain sentence", async () => {
  render(
    <UninstallDialog
      harness={agy}
      load={async () =>
        Promise.reject({ code: "invalid", message: "Antigravity isn't installed." })
      }
      onUninstall={vi.fn()}
      onClose={vi.fn()}
    />,
  );
  await act(async () => {});
  expect(screen.getByRole("alert").textContent).toBe("Antigravity isn't installed.");
});
