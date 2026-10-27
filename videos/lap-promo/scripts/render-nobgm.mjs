import { spawnSync } from "node:child_process";
import fs from "node:fs/promises";
import assert from "node:assert/strict";

await fs.access("out/lap-promo-4k.mp4");
const result = spawnSync(
  process.execPath,
  [
    "node_modules/@remotion/cli/remotion-cli.js",
    "render",
    "src/index.ts",
    "Lap-Promo-4K",
    "out/qa/nobgm-sfx.wav",
    "--props=props-nobgm.json",
    "--codec=wav",
    "--concurrency=1",
    "--timeout=120000",
  ],
  { stdio: "inherit" },
);
assert.equal(result.status, 0, "SFX render failed.");
await import("./lock-picture.mjs");
