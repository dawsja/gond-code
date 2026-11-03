import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";

// Lock the approved geometry independently of the replaceable hash pins.
const expected =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024"><g transform="translate(512 512) scale(1.1667) translate(-512 -512)"><rect x="128" y="128" width="768" height="768" rx="300" fill="#1b1a18"/><rect x="141" y="141" width="742" height="742" rx="287" fill="none" stroke="#ffffff" stroke-opacity="0.22" stroke-width="26"/><g transform="rotate(-16 380 540)"><path d="M325 450 L445 540 L325 630" fill="none" stroke="#ffffff" stroke-width="110" stroke-linecap="round" stroke-linejoin="round"/></g><rect x="584" y="340" width="120" height="288" rx="60" fill="#ffffff" transform="rotate(-16 644 484)"/></g></svg>';
const actual = readFileSync("docs/gond-code.svg", "utf8").replace(/>\s+</g, "><").trim();
if (actual !== expected) {
  throw new Error("Gond Code must keep the approved family frame, glyph, colours and transforms.");
}

const sources = execFileSync("git", ["ls-files", "-z"], { encoding: "utf8" })
  .split("\0")
  .filter((file) => /\.(svg|tsx?|jsx?|html|rs|css|json|md)$/.test(file));
const oldFrame = /\brx\s*=\s*["'](?:176|163)["']/;
for (const file of sources) {
  if (oldFrame.test(readFileSync(file, "utf8"))) {
    throw new Error(`Old Gond Code frame in ${file}`);
  }
}
process.stdout.write("Brand source: approved frame and glyph; no old frame in tracked sources.\n");
