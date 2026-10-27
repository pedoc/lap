import { openBrowser } from "@remotion/renderer";
import fs from "node:fs/promises";
import { pathToFileURL } from "node:url";
import path from "node:path";

const browser = await openBrowser("chrome");
try {
  const page = await browser.newPage({
    context: null,
    logLevel: "error",
    indent: false,
    pageIndex: 0,
    onBrowserLog: null,
    onLog: () => {},
  });
  await page.setViewport({ width: 1920, height: 1080, deviceScaleFactor: 1 });
  await page.goto({
    url: pathToFileURL(path.resolve("styleframes.html")).href,
    timeout: 30000,
  });
  await page.evaluate(async () => {
    await document.fonts.ready;
    await Promise.all([...document.images].map((im) => im.decode()));
  });
  for (let i = 0; i < 3; i++) {
    await page.evaluate((y) => window.scrollTo(0, y), 1112 * i);
    const {
      value: { data },
    } = await page
      ._client()
      .send("Page.captureScreenshot", {
        format: "png",
        captureBeyondViewport: false,
      });
    await fs.writeFile(
      `out/qa/styleframe-${i + 1}.png`,
      Buffer.from(data, "base64"),
    );
  }
} finally {
  await browser.close({ silent: true });
}
