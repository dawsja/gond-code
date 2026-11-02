// Builds gond-remote, Gond's terminal host for WSL and SSH machines (src-tauri/remote), for
// Linux x86_64 and aarch64: static musl binaries linked with Rust's own linker, so it builds on
// Windows with no C toolchain for Linux. `bun run dist:win` bundles them beside the app
// (src-tauri/tauri.remote.conf.json); a debug build finds them in remote/target.
//
// bun scripts/build-remote.ts            both
// bun scripts/build-remote.ts x86_64     one
import { spawnSync } from "node:child_process";
import { join } from "node:path";

const all = ["x86_64", "aarch64"];
const want = process.argv.slice(2);
const arches = want.length ? all.filter((a) => want.includes(a)) : all;
// Cargo reads remote/.cargo/config.toml (the linker, `--cfg gond_remote`) only from there.
const cwd = join(import.meta.dir, "..", "src-tauri", "remote");

for (const arch of arches) {
  const target = `${arch}-unknown-linux-musl`;
  const added = spawnSync("rustup", ["target", "add", target], { stdio: "inherit" });
  if (added.status !== 0) process.exit(added.status ?? 1);
  const built = spawnSync("cargo", ["build", "--release", "--target", target], {
    cwd,
    stdio: "inherit",
  });
  if (built.status !== 0) process.exit(built.status ?? 1);
  console.log(`gond-remote for ${arch}: src-tauri/remote/target/${target}/release/gond-remote`);
}
