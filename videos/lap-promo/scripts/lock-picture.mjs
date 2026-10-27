import { spawnSync } from "node:child_process";
import fs from "node:fs/promises";
import path from "node:path";
import assert from "node:assert/strict";

// Render the SFX soundtrack with bgm=false, then reuse the picture master.
const out = path.resolve("out");
const main = path.join(out, "lap-promo-4k.mp4");
const alternate = path.join(out, "lap-promo-4k-nobgm.mp4");
const frozen = path.join(out, "qa", "nobgm-sfx.wav");
const locked = path.join(out, "lap-promo-4k-nobgm-locked.mp4");
for (const file of [main, alternate, frozen, locked])
  assert.ok(file.startsWith(out + path.sep));
await fs.access(frozen);
const result = spawnSync(
  "ffmpeg",
  [
    "-v",
    "error",
    "-y",
    "-i",
    main,
    "-i",
    frozen,
    "-map",
    "0:v:0",
    "-map",
    "1:a:0",
    "-c:v",
    "copy",
    "-c:a",
    "aac",
    "-b:a",
    "320k",
    "-ar",
    "48000",
    "-ac",
    "2",
    "-movflags",
    "+faststart",
    locked,
  ],
  { stdio: "inherit" },
);
assert.equal(
  result.status,
  0,
  "Picture-lock remux failed; the original alternate render is retained.",
);
const pictureHash = (file) => {
  const result = spawnSync(
    "ffmpeg",
    [
      "-v",
      "error",
      "-i",
      file,
      "-map",
      "0:v:0",
      "-c",
      "copy",
      "-f",
      "hash",
      "-hash",
      "sha256",
      "-",
    ],
    { encoding: "utf8" },
  );
  assert.equal(result.status, 0);
  return result.stdout.trim();
};
assert.equal(
  pictureHash(main),
  pictureHash(locked),
  "The picture master must remain unchanged.",
);
await fs.rename(locked, alternate);
await fs.writeFile(
  path.join(out, "qa", "picture-lock.json"),
  JSON.stringify(
    {
      picture: "lap-promo-4k.mp4",
      soundtrack: "qa/nobgm-sfx.wav",
      soundtrackOrigin: "Remotion audio-only render with props-nobgm.json",
      picturePacketsUnchanged: true,
    },
    null,
    2,
  ),
);
console.log(
  "Alternate soundtrack preserved; both deliverables now use the exact same encoded picture.",
);
