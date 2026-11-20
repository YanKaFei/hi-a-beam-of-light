// Render the healing interface boards to PNG/WebP for the READMEs.
// Usage: node render.mjs [--dpr 2]
import fs from "node:fs/promises";
import path from "node:path";
import { createRequire } from "node:module";

// Resolve puppeteer-core from the project first, then from an explicit path.
// export PUPPETEER_CORE=/path/to/puppeteer-core/lib/esm/puppeteer/puppeteer-core.js
const require = createRequire(import.meta.url);
const explicit = process.env.PUPPETEER_CORE;
const puppeteer = explicit
  ? (await import(explicit)).default
  : require("puppeteer-core");

const CHROME =
  process.env.CHROME_PATH ||
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const HERE = path.dirname(new URL(import.meta.url).pathname);
const OUT = path.resolve(HERE, "../../assets/readme");

const args = process.argv.slice(2);
const dprArg = args.indexOf("--dpr");
const DPR = dprArg > -1 ? Number(args[dprArg + 1]) : 2;

const LANGS = ["zh", "en", "fr", "ja"];
const SCREENS = [
  ["home", 390, 844],
  ["scan", 390, 844],
  ["transfer", 390, 844],
  ["path", 390, 844],
  ["echo", 390, 844],
  ["moment", 390, 844],
  ["hero", 1600, 760],
];

async function shoot(page, lang, frame, w, h, dpr, file) {
  await page.setViewport({ width: w, height: h, deviceScaleFactor: dpr });
  const url = `file://${path.join(HERE, "index.html")}?lang=${lang}&frame=${frame}`;
  await page.goto(url, { waitUntil: "load" });
  await page.waitForFunction(() => document.body.classList.contains("capture"), { timeout: 8000 });
  await page.evaluate(() => document.fonts.ready);
  await new Promise((r) => setTimeout(r, 250));
  const el = await page.$("#frame-" + frame);
  await el.screenshot({ path: file, type: "png" });
  const { size } = await fs.stat(file);
  console.log(`${file.replace(OUT + "/", "")}  ${w}x${h}@${dpr}  ${(size / 1024).toFixed(0)} KB`);
}

const browser = await puppeteer.launch({
  executablePath: CHROME,
  headless: "new",
  args: ["--font-render-hinting=none", "--force-color-profile=srgb", "--hide-scrollbars"],
});

try {
  const page = await browser.newPage();
  await page.setDefaultTimeout(20000);

  for (const lang of LANGS) {
    const dir = path.join(OUT, lang);
    await fs.mkdir(dir, { recursive: true });
    for (const [frame, w, h] of SCREENS) {
      const dpr = frame === "hero" ? 1 : DPR;
      await shoot(page, lang, frame, w, h, dpr, path.join(dir, `${frame}.png`));
    }
  }

  const shared = path.join(OUT, "shared");
  await fs.mkdir(shared, { recursive: true });
  await shoot(page, "zh", "levels", 1560, 660, 1, path.join(shared, "levels.png"));
} finally {
  await browser.close();
}
