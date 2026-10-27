import { bundle } from "@remotion/bundler";
import {
  openBrowser,
  selectComposition,
  renderStill,
} from "@remotion/renderer";
import fs from "node:fs/promises";
import path from "node:path";

const serveUrl = await bundle({
  entryPoint: path.resolve("src/index.ts"),
  outDir: path.resolve("out/bundle"),
});
console.log("Bundle ready");
const browser = await openBrowser("chrome");
try {
  const composition = await selectComposition({
    serveUrl,
    id: "Lap-Promo-4K",
    inputProps: { bgm: true },
    puppeteerInstance: browser,
  });
  const frames = [
    0, 18, 54, 97, 125, 162, 207, 238, 267, 304, 315, 335, 372, 400, 437, 483,
    514, 550, 594, 623, 649, 677, 707, 735, 760, 790, 830, 885,
  ];
  for (let i = 0; i < frames.length; i++) {
    const frame = frames[i];
    await renderStill({
      serveUrl,
      composition,
      inputProps: { bgm: true },
      puppeteerInstance: browser,
      output: `out/qa/frame-${String(frame).padStart(4, "0")}.png`,
      frame,
      scale: 0.5,
      imageFormat: "png",
      logLevel: "error",
      timeoutInMilliseconds: 60000,
    });
    console.log(`QA ${i + 1}/${frames.length}`);
  }
  await fs.writeFile("out/qa/frames.json", JSON.stringify(frames));
} finally {
  await browser.close({ silent: true });
}
