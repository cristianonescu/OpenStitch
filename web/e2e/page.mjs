// Browser test of the web editor's page (#694): Chromium, Firefox and WebKit (Safari's
// engine) through Playwright. CI job `web` runs it after the browser build:
//   node web/e2e/page.mjs [--optional BROWSER]... WEB_DIR NATIVE_DIR INPUT [OUT_DIR]
// WEB_DIR holds index.html and pkg/ (wasm-bindgen --target web). In each browser the page
// must start; then "Open…" is clicked and INPUT given to the browser's file picker, and
// "Export" (PES, the default format) is clicked; the download must be byte-identical to
// NATIVE_DIR/<input name>.pes (`cargo run -p openstitch-web --example web_export`). DST goes
// through openstitch.run and must match NATIVE_DIR/<input name>.dst. A screenshot per browser
// goes to OUT_DIR when given. A browser named with --optional (chromium, firefox, webkit) that
// cannot be launched (a missing system library) is reported as a CI warning, not a failure;
// once it runs, every check counts.
import { createServer } from "node:http";
import { mkdirSync, readFileSync } from "node:fs";
import { basename, extname, join, normalize, resolve } from "node:path";
import { chromium, firefox, webkit } from "playwright";

const args = process.argv.slice(2);
const optional = new Set();
while (args[0] === "--optional") {
  args.shift();
  optional.add((args.shift() ?? "").toLowerCase());
}
const [webDir, nativeDir, input, outDir] = args;
if (!webDir || !nativeDir || !input) {
  console.error("usage: node web/e2e/page.mjs [--optional BROWSER]... WEB_DIR NATIVE_DIR INPUT [OUT_DIR]");
  process.exit(2);
}
const root = resolve(webDir);
const name = basename(input);
const types = { ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm" };

// A static server for WEB_DIR on a free loopback port.
const server = createServer((req, res) => {
  const path = normalize(decodeURIComponent(new URL(req.url, "http://x").pathname));
  const file = join(root, path.endsWith("/") ? `${path}index.html` : path);
  if (!file.startsWith(root)) return res.writeHead(403).end();
  try {
    const body = readFileSync(file);
    res.writeHead(200, { "content-type": types[extname(file)] ?? "application/octet-stream" });
    res.end(body);
  } catch {
    res.writeHead(404).end();
  }
});
await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
const url = `http://127.0.0.1:${server.address().port}/`;

// The file bar's buttons on the egui canvas at 1100 × 750 and scale 1 (egui's own fonts,
// so the same in every browser): "Open…" and "Export".
const OPEN = { x: 37, y: 12 };
const EXPORT = { x: 277, y: 12 };

let failures = 0;
for (const [label, type] of [["Chromium", chromium], ["Firefox", firefox], ["WebKit", webkit]]) {
  console.log(`${label}: launching`);
  let browser;
  try {
    browser = await type.launch({ timeout: 60_000 });
  } catch (e) {
    const first = String(e.message).split("\n").slice(0, 3).join(" ");
    if (optional.has(label.toLowerCase())) {
      console.log(`::warning::${label} could not be launched on this runner, not tested: ${first}`);
    } else {
      console.error(`FAIL ${label}: could not be launched: ${first}`);
      failures += 1;
    }
    continue;
  }
  try {
    const page = await browser.newPage({ viewport: { width: 1100, height: 750 }, deviceScaleFactor: 1, acceptDownloads: true });
    const errors = [];
    page.on("pageerror", (e) => errors.push(String(e)));
    await page.goto(url, { timeout: 60_000 });
    console.log(`${label}: page loaded, waiting for the editor to start`);
    await page.waitForFunction(
      () => !document.getElementById("message") || /could not start/.test(document.getElementById("message").textContent),
      null,
      { timeout: 90_000 },
    );
    const message = await page.evaluate(() => document.getElementById("message")?.textContent ?? "");
    if (message) throw new Error(message);
    await page.waitForTimeout(500);

    console.log(`${label}: started; opening ${name} through the file picker`);
    // File ▸ Open through the real picker.
    const [chooser] = await Promise.all([page.waitForEvent("filechooser", { timeout: 15_000 }), page.mouse.click(OPEN.x, OPEN.y)]);
    await chooser.setFiles(input);
    await page.waitForFunction(
      (n) => window.openstitch.run({ command: "ui.state" }).result.file === n,
      name,
      { timeout: 30_000 },
    );

    console.log(`${label}: opened; exporting PES through a download`);
    // File ▸ Export through the real download.
    const [download] = await Promise.all([page.waitForEvent("download", { timeout: 15_000 }), page.mouse.click(EXPORT.x, EXPORT.y)]);
    const got = readFileSync(await download.path());
    const want = readFileSync(join(nativeDir, `${name}.pes`));
    if (!got.equals(want) || !download.suggestedFilename().endsWith(".pes")) {
      throw new Error(`downloaded ${download.suggestedFilename()} (${got.length} bytes), native ${want.length} bytes`);
    }

    // The automation API in the same page.
    const dst = await page.evaluate(() => window.openstitch.run({ command: "ui.exportFile", params: { format: "dst" } }));
    if (!dst.ok || !Buffer.from(dst.result.files[0].base64, "base64").equals(readFileSync(join(nativeDir, `${name}.dst`)))) {
      throw new Error(`ui.exportFile dst: ${JSON.stringify(dst).slice(0, 200)}`);
    }
    if (outDir) {
      mkdirSync(outDir, { recursive: true });
      await page.screenshot({ path: join(outDir, `web-${label.toLowerCase()}.png`) });
    }
    if (errors.length) throw new Error(`page errors: ${errors.join("; ")}`);
    console.log(`ok ${label}: started, opened ${name} with the file picker, the PES download and the DST export are identical to the desktop build`);
  } catch (e) {
    console.error(`FAIL ${label}: ${e.message}`);
    failures += 1;
  } finally {
    await browser.close().catch(() => {});
  }
}
server.closeAllConnections?.();
server.close();
if (failures) process.exit(1);
console.log("web page test passed");
process.exit(0);
