import { expect, it } from "vitest";
import { promptFolder } from "./prompt";

// A shell pane reopens after an update in the folder its prompt showed.
it("reads the folder from PowerShell, Command Prompt and Git Bash prompts", () => {
  expect(promptFolder(["", "PS C:\\Users\\d\\src\\gond-code> "])).toBe(
    "C:\\Users\\d\\src\\gond-code",
  );
  expect(promptFolder(["PS C:\\> git status", "..."])).toBe("C:\\");
  expect(promptFolder(["Microsoft Windows", "", "D:\\work>dir"])).toBe("D:\\work");
  expect(promptFolder(["sam@PC MINGW64 /c/Users/sam/src (main)", "$ "])).toBe(
    "C:\\Users\\sam\\src",
  );
  expect(promptFolder(["sam@PC MINGW64 /d", "$ "])).toBe("D:\\");
  expect(promptFolder(["user@pc:~/src$ "])).toBeUndefined();
  expect(promptFolder(["Compiling gond-code v0.1.0"])).toBeUndefined();
});
