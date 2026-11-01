// A pretend PowerShell for the browser mock: echoes keys, answers a few commands, so the
// terminal panel can be tried without Tauri. Real shells come from pty_open (ConPTY).

const RESET = "\x1b[0m";
const DIM = "\x1b[2m";
const GREEN = "\x1b[32m";
const BLUE = "\x1b[34m";
const RED = "\x1b[31m";

type Shell = {
  cwd: string;
  line: string;
  emit: (data: string) => void;
  exit: (code: number) => void;
};

const prompt = (s: Shell) => `${BLUE}PS${RESET} ${s.cwd}> `;

const COMMANDS: Record<string, (s: Shell, args: string) => string> = {
  help: () =>
    `Try ${GREEN}git status${RESET}, ${GREEN}ls${RESET}, ${GREEN}echo${RESET}, ${GREEN}clear${RESET} or ${GREEN}exit${RESET}.\r\n`,
  ls: () =>
    ["src/", "docs/", "package.json", "README.md"]
      .map((f) => (f.endsWith("/") ? `${BLUE}${f}${RESET}` : f))
      .join("  ") + "\r\n",
  "git status": (s) =>
    `On branch gond/${s.cwd.split("\\").pop()}\r\nChanges not staged for commit:\r\n  ${RED}modified:   src/main.tsx${RESET}\r\n`,
  echo: (_, args) => `${args}\r\n`,
  clear: () => "\x1b[2J\x1b[H",
};
COMMANDS.dir = COMMANDS.ls;

export function startShell(cwd: string, emit: Shell["emit"], exit: Shell["exit"]) {
  const s: Shell = { cwd, line: "", emit, exit };
  emit(`${DIM}PowerShell 7.5 (mock shell in the browser)${RESET}\r\n${prompt(s)}`);
  return (data: string) => {
    for (const ch of data) {
      if (ch === "\r") {
        const line = s.line.trim();
        s.line = "";
        emit("\r\n");
        if (line === "exit") return exit(0);
        const name = Object.keys(COMMANDS).find((c) => line === c || line.startsWith(`${c} `));
        if (name) emit(COMMANDS[name](s, line.slice(name.length).trim()));
        else if (line)
          emit(
            `${RED}${line.split(" ")[0]}: not in the mock shell.${RESET} Type ${GREEN}help${RESET}.\r\n`,
          );
        emit(prompt(s));
      } else if (ch === "\x7f") {
        if (s.line) {
          s.line = s.line.slice(0, -1);
          emit("\b \b");
        }
      } else if (ch >= " ") {
        s.line += ch;
        emit(ch);
      }
    }
  };
}
