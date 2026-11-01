// Gond Code Dev with WebView2's DevTools port open (127.0.0.1:9223, or GOND_CDP_PORT), so an
// agent can drive its UI with scripts/dev-cdp.ts without touching the real mouse or keyboard.
// It's `bun run tauri dev` plus one browser argument; it never starts or touches prod.
//
// bun run dev:debug
import { spawn } from "node:child_process";
import { tmpdir } from "node:os";
import { join } from "node:path";

const port = process.env.GOND_CDP_PORT ?? "9223";
const root = join(import.meta.dir, "..");
const conf = (await Bun.file(join(root, "src-tauri/tauri.conf.json")).json()) as {
  app: { windows: { additionalBrowserArgs?: string }[] };
};
// A JSON merge patch replaces arrays whole, so every window goes in with its own settings.
const windows = conf.app.windows.map((w) => ({
  ...w,
  additionalBrowserArgs: [
    w.additionalBrowserArgs ?? "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection",
    `--remote-debugging-port=${port}`,
  ].join(" "),
}));
const patch = join(tmpdir(), "gond-code-dev-debug.json");
await Bun.write(patch, JSON.stringify({ app: { windows } }));
console.log(`Gond Code Dev, DevTools on 127.0.0.1:${port}`);
const child = spawn("bun", ["x", "tauri", "dev", "--config", patch], {
  cwd: root,
  stdio: "inherit",
  shell: process.platform === "win32",
});
child.on("exit", (code) => process.exit(code ?? 0));
