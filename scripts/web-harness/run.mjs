// Run one console script in the browser version of the game and collect
// what the harness judges: the console, the screenshots, panics.
// Started by scripts/web-harness.sh, which also judges the result.
//
//   bun run.mjs <page url> <out dir> <timeout secs> <width>x<height>
//
// The game prints every console line to the browser console with a
// "[fast-fps] " prefix (console/mod.rs); those become console.txt. Other
// browser messages go to browser.log. `screenshot` saves are downloads.

import { chromium } from "playwright-core";
import { appendFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const [url, out, timeoutArg, size] = process.argv.slice(2);
const timeout = Number(timeoutArg) * 1000;
const [width, height] = size.split("x").map(Number);
const PREFIX = "[fast-fps] ";

writeFileSync(join(out, "console.txt"), "");
writeFileSync(join(out, "browser.log"), "");
const log = (file, line) => appendFileSync(join(out, file), line + "\n");

const options = {
  executablePath: process.env.CHROME ?? "/usr/bin/google-chrome-stable",
  headless: false,
  args: [
    "--ozone-platform=wayland",
    "--enable-unsafe-webgpu",
    "--enable-features=Vulkan,WebGPU",
    `--window-size=${width},${height}`,
    // Let the page play sound without a real click.
    "--autoplay-policy=no-user-gesture-required",
  ],
  viewport: { width, height },
  acceptDownloads: true,
};
// PROFILE_DIR keeps the browser's storage between runs (web-harness.sh -c),
// to test that settings survive a reload.
const context = await chromium.launchPersistentContext(
  process.env.PROFILE_DIR ?? "",
  options,
);
const browser = context;
const page = context.pages()[0] ?? (await context.newPage());

let quit = false;
page.on("console", (msg) => {
  const text = msg.text();
  if (text.startsWith(PREFIX)) {
    const line = text.slice(PREFIX.length);
    log("console.txt", line);
    if (line === "] quit") quit = true;
  } else {
    log("browser.log", `${msg.type()}: ${text}`);
  }
});
page.on("pageerror", (e) => log("browser.log", `pageerror: ${e.message}`));
page.on("download", async (download) => {
  await download.saveAs(join(out, download.suggestedFilename()));
});

await page.goto(url);
const deadline = Date.now() + timeout;
while (!quit && Date.now() < deadline) {
  await page.waitForTimeout(200);
}
// Let a last screenshot download finish.
await page.waitForTimeout(quit ? 1500 : 0);
log("browser.log", quit ? "reached quit" : `timed out after ${timeoutArg}s`);
await browser.close();
process.exit(quit ? 0 : 3);
