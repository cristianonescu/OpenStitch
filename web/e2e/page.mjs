// Browser test of the web editor's page (#694): Chromium, Firefox and WebKit (Safari's
// engine) through Playwright. CI job `web` runs it after the browser build:
//   node web/e2e/page.mjs [--optional BROWSER]... WEB_DIR NATIVE_DIR INPUT [OUT_DIR]
// WEB_DIR holds index.html and pkg/ (wasm-bindgen --target web). In each browser the page
// must start the desktop editor (#693); then File ▸ Open (Ctrl+O) gives INPUT to the browser's
// file picker, and the design is exported as PES and DST through openstitch.run into the
// editor's save folder (/web/save), which the page hands to the browser as downloads; each
// must be byte-identical to NATIVE_DIR/<input name>.pes and .dst (`cargo run -p openstitch-web
// --example web_export`). A screenshot per browser
// goes to OUT_DIR when given. A browser named with --optional (chromium, firefox, webkit) that
// cannot be launched (a missing system library) or has no WebGL on the machine is reported as a
// CI warning, not a failure; once the editor has started in it, every check counts. Firefox may
// use software GL (Mesa's llvmpipe), which it otherwise blocks.
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

const stem = name.replace(/\.[^.]*$/, "");
const run = (page, request) => page.evaluate((r) => window.openstitch.run(r), request);
// Waits until the editor has started (or says it could not); returns the page's message.
const started = async (page) => {
  await page.waitForFunction(
    () => !document.getElementById("message") || /could not start/.test(document.getElementById("message").textContent),
    null,
    { timeout: 90_000 },
  );
  return page.evaluate(() => document.getElementById("message")?.textContent ?? "");
};

let failures = 0;
for (const [label, type] of [["Chromium", chromium], ["Firefox", firefox], ["WebKit", webkit]]) {
  console.log(`${label}: launching`);
  let browser;
  try {
    browser = await type.launch({
      timeout: 60_000,
      // Linux CI: headless Firefox has no WebGL there, so it runs on Xvfb's display instead.
      headless: !(type === firefox && process.env.HEADED_FIREFOX === "1"),
      ...(type === firefox ? { firefoxUserPrefs: { "webgl.force-enabled": true } } : {}),
    });
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
    const message = await started(page);
    if (message && /WebGL/i.test(message) && optional.has(label.toLowerCase())) {
      console.log(`::warning::${label} has no WebGL on this runner, not tested: ${message}`);
      continue;
    }
    if (message) throw new Error(message);
    await page.waitForTimeout(500);

    console.log(`${label}: started; opening ${name} through the file picker (Ctrl+O)`);
    // File ▸ Open's shortcut on the focused canvas; the editor asks the page for the picker.
    await page.evaluate(() => document.getElementById("openstitch").focus());
    const [chooser] = await Promise.all([
      page.waitForEvent("filechooser", { timeout: 15_000 }),
      page.keyboard.press("Control+O"),
    ]);
    await chooser.setFiles(input);
    await page.waitForFunction(
      () => {
        const s = window.openstitch.run({ command: "ui.state" }).result;
        return s && s.loading == null && s.stats.stitches > 0;
      },
      null,
      { timeout: 30_000 },
    );

    // Exports into the save folder become downloads.
    for (const format of ["pes", "dst"]) {
      console.log(`${label}: opened; exporting ${format.toUpperCase()} through a download`);
      const file = `${stem}.${format}`;
      const [download, reply] = await Promise.all([
        page.waitForEvent("download", { timeout: 15_000 }),
        run(page, { command: "design.export", params: { path: `/web/save/${file}`, format } }),
      ]);
      if (!reply.ok) throw new Error(`design.export ${format}: ${JSON.stringify(reply).slice(0, 200)}`);
      const got = readFileSync(await download.path());
      const want = readFileSync(join(nativeDir, `${name}.${format}`));
      if (!got.equals(want) || download.suggestedFilename() !== file) {
        throw new Error(`downloaded ${download.suggestedFilename()} (${got.length} bytes), native ${want.length} bytes`);
      }
    }
    // #797: preferences last in the page's storage. "Don't show this again" (showStartCards
    // off) survives a reload, and so does turning the cards back on.
    for (const value of [false, true]) {
      console.log(`${label}: start cards ${value ? "on" : "off"}, then a reload`);
      const set = await run(page, { command: "ui.set", params: { name: "showStartCards", value } });
      if (!set.ok) throw new Error(`ui.set showStartCards: ${JSON.stringify(set).slice(0, 200)}`);
      await page.reload({ timeout: 60_000 });
      const again = await started(page);
      if (again) throw new Error(`after a reload: ${again}`);
      const prefs = await run(page, { command: "ui.settings" });
      if (prefs.result?.showStartCards?.value !== value) {
        throw new Error(`showStartCards is ${JSON.stringify(prefs.result?.showStartCards)} after a reload, want ${value}`);
      }
    }
    if (outDir) {
      mkdirSync(outDir, { recursive: true });
      await page.screenshot({ path: join(outDir, `web-${label.toLowerCase()}.png`) });
    }
    if (errors.length) throw new Error(`page errors: ${errors.join("; ")}`);
    console.log(`ok ${label}: started, opened ${name} with the file picker, the PES and DST downloads are identical to the desktop build, preferences last across reloads`);
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
