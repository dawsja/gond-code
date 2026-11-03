import { expect, it } from "vitest";
import { folderForPane, setPaneFolder } from "./paneFolder";
import type { Pane } from "./layout";
import { promptFolder } from "./prompt";

it("opens the pane's current folder before its resume or registered start folder", () => {
  const pane: Pane = { kind: "pane", id: "folder-test", projectId: "project" };
  const projects = [{ id: "project", path: "C:\\start" }];
  expect(folderForPane(pane, projects)).toBe("C:\\start");
  pane.resume = { agent: "claude", command: "claude", cwd: "C:\\resume" };
  expect(folderForPane(pane, projects)).toBe("C:\\resume");
  setPaneFolder(pane.id, "C:\\plain");
  expect(folderForPane(pane, projects)).toBe("C:\\plain");
  pane.projectId = "";
  expect(folderForPane(pane, projects)).toBe("C:\\plain");
  setPaneFolder(pane.id, undefined);
  delete pane.resume;
  expect(folderForPane(pane, projects)).toBe("~");
  pane.projectId = "removed";
  expect(folderForPane(pane, projects)).toBe("~");
  for (const shell of ["ssh:host", "wsl:Ubuntu"]) {
    pane.shell = shell;
    expect(folderForPane(pane, projects)).toBeUndefined();
  }
});

it("reads forward-slash drive and UNC prompts without losing roots or path case", () => {
  expect(promptFolder(["PS c:/Users/X/>"])).toBe("c:/Users/X");
  expect(promptFolder(["PS c:/> "])).toBe("c:/");
  expect(promptFolder(["PS \\\\server\\share\\folder> "])).toBe("\\\\server\\share\\folder");
  expect(promptFolder(["PS \\\\wsl$\\Ubuntu\\home\\x> "])).toBe("\\\\wsl$\\Ubuntu\\home\\x");
});

it("discards prompt-derived UNC paths and uses trusted folder fallbacks", () => {
  const pane: Pane = { kind: "pane", id: "unc", projectId: "project" };
  const projects = [{ id: "project", path: "C:\\start" }];
  for (const path of ["\\\\evil\\share", "\\\\wsl$\\Ubuntu\\home", "//evil/share"]) {
    setPaneFolder(pane.id, path);
    expect(folderForPane(pane, projects)).toBe("C:\\start");
    pane.resume = { agent: "claude", command: "claude", cwd: "C:\\resume" };
    expect(folderForPane(pane, projects)).toBe("C:\\resume");
    delete pane.resume;
    expect(folderForPane(pane, [])).toBe("~");
  }
});
