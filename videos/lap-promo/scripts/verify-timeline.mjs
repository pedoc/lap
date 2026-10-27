import { build } from "esbuild";
import { pathToFileURL } from "node:url";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";

await build({
  entryPoints: ["src/Composition.tsx"],
  bundle: true,
  platform: "node",
  packages: "external",
  format: "esm",
  outfile: "out/qa/timeline.mjs",
});
const { SHOTS, SFX_CUES, TOTAL, beatF } = await import(
  pathToFileURL(path.resolve("out/qa/timeline.mjs"))
);
const grid = JSON.parse(await fs.readFile("out/qa/music-grid.json", "utf8"));
assert.equal(new Set(SHOTS.map((s) => s.id)).size, SHOTS.length);
assert.equal(SHOTS[0].from, 0);
for (let i = 1; i < SHOTS.length; i++)
  assert.equal(SHOTS[i].from, SHOTS[i - 1].from + SHOTS[i - 1].duration);
assert.equal(SHOTS.at(-1).from + SHOTS.at(-1).duration, TOTAL);
assert.equal(TOTAL, grid.totalFrames);
assert.deepEqual(
  SHOTS.map((s) => s.from),
  grid.cutBeatNumbers.slice(0, -1).map(beatF),
);
assert.ok(SFX_CUES.every((s) => s.from >= 0 && s.from + s.duration <= TOTAL));
await fs.writeFile(
  "out/qa/timeline.json",
  JSON.stringify(
    {
      total: TOTAL,
      fps: 30,
      shots: SHOTS.map(({ component, ...s }) => s),
      sfx: SFX_CUES,
    },
    null,
    2,
  ),
);
console.log(
  `Timeline: contiguous ${TOTAL} frames; ${SHOTS.length} independent scenes; all cuts match the measured beat grid.`,
);
