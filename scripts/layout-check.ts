// Layout check: drives the running app's WebView2 (or any Chrome) over the DevTools
// protocol. For every screen in e2e/layout/screens.json at every size, it sets the viewport,
// opens the screen and runs e2e/layout/audit.js; it saves no images. It first proves the audit
// still catches a deliberately cramped page (e2e/layout/cramped.html).
//
// bun scripts/layout-check.ts --port 9222 [--base http://tauri.localhost] [--out layout] [--screens a,b]
// Writes <out>/results.json and <out>/summary.md; exits 1 on any violation.
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const arg = (name: string, fallback?: string) => {
  const i = process.argv.indexOf(`--${name}`);
  return i > 0 ? process.argv[i + 1] : fallback;
};
const port = arg("port", "9222");
const out = arg("out", "layout")!;
const root = join(import.meta.dir, "..");
const audit = readFileSync(join(root, "e2e/layout/audit.js"), "utf8");
const cramped = readFileSync(join(root, "e2e/layout/cramped.html"), "utf8");
const config = JSON.parse(readFileSync(join(root, "e2e/layout/screens.json"), "utf8")) as {
  screens: Record<string, string>;
  sizes: [number, number][];
};
const only = arg("screens")?.split(",");
const screens = Object.entries(config.screens).filter(([name]) => !only || only.includes(name));
// How many Gond marks the page may render: 1, or 0 under Windows' own caption.
const maxMarks = Number(arg("max-marks", "1"));
// The native title bar takes about 32px of the window; the page gets the rest.
const CAPTION = Number(arg("caption", "32"));
mkdirSync(out, { recursive: true });

type Violation = { rule: string; where: string; text: string; detail: string };
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

// Find the app's page target.
let target: { webSocketDebuggerUrl: string; url: string } | undefined;
for (let i = 0; i < 120 && !target; i++) {
  try {
    const host = i % 2 ? "[::1]" : "127.0.0.1";
    const list = (await (await fetch(`http://${host}:${port}/json`)).json()) as {
      type: string;
      url: string;
      webSocketDebuggerUrl: string;
    }[];
    // The app's page, once it has loaded (WebView2 starts on about:blank).
    target = list.find((t) => t.type === "page" && /^https?:/.test(t.url));
  } catch {
    /* not up yet */
  }
  if (!target) await sleep(500);
}
if (!target) throw new Error(`No app page on the DevTools port ${port}`);
const base = arg("base", new URL(target.url).origin)!;
console.log(`Page ${target.url}; screens from ${base}`);

const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((r, j) => ((ws.onopen = r), (ws.onerror = j)));
let seq = 0;
const pending = new Map<number, (v: { result?: unknown; error?: { message: string } }) => void>();
ws.onmessage = (e) => {
  const msg = JSON.parse(String(e.data));
  if (msg.id && pending.has(msg.id)) pending.get(msg.id)!(msg);
};
async function send<T = Record<string, unknown>>(method: string, params: object = {}): Promise<T> {
  const id = ++seq;
  ws.send(JSON.stringify({ id, method, params }));
  const msg = await new Promise<{ result?: unknown; error?: { message: string } }>((r) =>
    pending.set(id, r),
  );
  pending.delete(id);
  if (msg.error) throw new Error(`${method}: ${msg.error.message}`);
  return msg.result as T;
}
async function runAudit(): Promise<Violation[]> {
  const r = await send<{ result: { value: Violation[] }; exceptionDetails?: { text: string } }>(
    "Runtime.evaluate",
    {
      expression: `window.__GOND_MAX_MARKS = ${maxMarks};\n${audit}`,
      returnByValue: true,
    },
  );
  if (r.exceptionDetails) throw new Error(`audit threw: ${JSON.stringify(r.exceptionDetails)}`);
  return r.result.value;
}
async function size(w: number, h: number) {
  await send("Emulation.setDeviceMetricsOverride", {
    width: w,
    height: h - CAPTION,
    deviceScaleFactor: 1,
    mobile: false,
  });
}
async function evaluate<T>(expression: string): Promise<T> {
  const r = await send<{ result: { value: T } }>("Runtime.evaluate", {
    expression,
    returnByValue: true,
  });
  return r.result.value;
}
/** Polls a page condition until it's true or `ms` runs out; returns whether it came true. */
async function waitFor(expression: string, ms: number): Promise<boolean> {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    try {
      if (await evaluate<boolean>(`!!(${expression})`)) return true;
    } catch {
      /* the page is between documents */
    }
    await sleep(100);
  }
  return false;
}
/** What the page looks like right now, for failure messages. */
const pageState = () =>
  evaluate<Record<string, unknown>>(
    `({ url: location.href, readyState: document.readyState, title: document.title, bodyChars: (document.body && document.body.innerText || "").length, fixture: !!document.getElementById("overflow"), width: innerWidth })`,
  );
async function open(search: string) {
  await send("Page.navigate", { url: `${base}/${search}` });
  await waitFor(`document.readyState === "complete"`, 15_000);
  await sleep(2500); // the mock's calls settle and the layout finishes
}

await send("Page.enable");
await send("Runtime.enable");
await sleep(2000); // let the app finish its first render before the fixture replaces it

// 1. The audit must catch every rule on the cramped fixture, or the check itself is broken.
await size(1024, 768);
// The fixture is loaded in place of the app; wait until its own elements exist and are laid out
// (not a fixed delay), and try once more from scratch if the first load didn't take.
const FIXTURE_READY = `document.readyState === "complete" && document.getElementById("overflow") && document.getElementById("overflow").getBoundingClientRect().width > 0`;
async function loadFixture(): Promise<boolean> {
  const { frameTree } = await send<{ frameTree: { frame: { id: string } } }>("Page.getFrameTree");
  await send("Page.setDocumentContent", { frameId: frameTree.frame.id, html: cramped });
  return waitFor(FIXTURE_READY, 10_000);
}
let fixture: Violation[] = [];
for (let attempt = 1; attempt <= 2; attempt++) {
  const ready = await loadFixture();
  fixture = ready ? await runAudit() : [];
  if (ready && fixture.length) break;
  console.warn(
    `Self-test attempt ${attempt}: fixture ${ready ? "loaded but no findings" : "didn't load"}; page: ${JSON.stringify(await pageState())}`,
  );
}
const RULES = [
  "overflow",
  "clipped",
  "no-tooltip",
  "page-x",
  "nested-scroll",
  "word-per-line",
  "narrow-text",
  "min-width",
  "logo-count",
  "concentric",
  "control-clipped",
  "header-centred",
  "label-wrap",
];
const missed = RULES.filter((r) => !fixture.some((v) => v.rule === r));
const falseAlarms = fixture.filter((v) => /Fine|deliberate/.test(v.text));
console.log(
  `Self-test on cramped.html: ${fixture.length} findings; missed ${missed.join(", ") || "none"}`,
);
if (missed.length || falseAlarms.length) {
  console.error(JSON.stringify({ missed, falseAlarms, fixture, page: await pageState() }, null, 2));
  throw new Error("The layout audit no longer catches the cramped fixture");
}

// 2. Every screen at every size.
const results: { screen: string; size: string; violations: Violation[] }[] = [];
for (const [w, h] of config.sizes) {
  await size(w, h);
  for (const [name, search] of screens) {
    await open(search);
    const violations = await runAudit();
    results.push({ screen: name, size: `${w}x${h}`, violations });
    console.log(
      `${name} @ ${w}x${h}: ${violations.length ? violations.length + " problems" : "ok"}`,
    );
    for (const v of violations) console.log(`  ${v.rule}: ${v.where} "${v.text}" (${v.detail})`);
  }
}
ws.close();

writeFileSync(join(out, "results.json"), JSON.stringify(results, null, 2));
const failed = results.filter((r) => r.violations.length);
const md = [
  failed.length
    ? `❌ **${failed.reduce((n, r) => n + r.violations.length, 0)} layout problems** on ${failed.length} of ${results.length} screen×size renders.`
    : `✅ All ${results.length} screen×size renders pass the layout bar.`,
  "",
  ...failed.flatMap((r) => [
    `<details><summary><b>${r.screen}</b> at ${r.size}: ${r.violations.length}</summary>`,
    "",
    ...r.violations.map(
      (v) => `- \`${v.rule}\` ${v.where}${v.text ? ` “${v.text}”` : ""}: ${v.detail}`,
    ),
    "",
    "</details>",
  ]),
].join("\n");
writeFileSync(join(out, "summary.md"), md + "\n");
process.exit(failed.length ? 1 : 0);
