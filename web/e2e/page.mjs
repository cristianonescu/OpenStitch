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
// Then, each in its own browser context: a ready-to-sew sample opens (#833); Ctrl+S and Ctrl+P
// reach the editor, not the browser (#834); the editor draws at 2 device pixels per CSS pixel
// and works at a phone's width with touch (#834); a design left unsaved in a closed tab is
// offered and restored in the next one from IndexedDB (#831); and with the page's storage
// refused the editor still works on its defaults (#831); and the Point tool's right-click
// menus open in the editor while the browser's own menu stays shut, with the arrow keys and
// Delete kept in the editor (#1260). ONLY_BROWSER=Chromium (or Firefox, WebKit) runs one
// browser.
import { createServer } from "node:http";
import { mkdirSync, readFileSync } from "node:fs";
import { basename, dirname, extname, join, normalize, resolve } from "node:path";
import { fileURLToPath } from "node:url";
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
// A photo for the image tools (#797): a kit preview PNG from the repository.
const photo = resolve(dirname(fileURLToPath(import.meta.url)), "../../examples/sew/previews/applique.png");
const photoB64 = readFileSync(photo).toString("base64");
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


// A new page in `context` with its page errors collected; it waits until the editor has started.
const editorPage = async (context) => {
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => { if (/panicked at/.test(m.text())) errors.push(m.text().split("\n").slice(0, 2).join(" ")); });
  await page.goto(url, { timeout: 60_000 });
  const message = await started(page);
  if (message) throw new Error(message);
  await page.waitForTimeout(300);
  return { page, errors };
};
const noErrors = (what, errors) => {
  if (errors.length) throw new Error(`${what}: page errors: ${errors.join("; ")}`);
};
const objectCount = async (page) => (await run(page, { command: "object.list" })).result?.length ?? 0;

// A finger dragged from `from` to `to` on the canvas (#834), in page points. Chromium gets real
// touch input through its DevTools protocol; Firefox gets TouchEvents built in the page, as a
// touch screen sends them. WebKit builds neither (no Touch constructor on the desktop, no
// protocol for it), so there only Playwright's taps are used.
async function touchDrag(page, type, from, to) {
  const steps = 8;
  const at = (i) => [from[0] + ((to[0] - from[0]) * i) / steps, from[1] + ((to[1] - from[1]) * i) / steps];
  if (type === chromium) {
    const cdp = await page.context().newCDPSession(page);
    const send = (kind, [x, y]) => cdp.send("Input.dispatchTouchEvent", {
      type: kind,
      touchPoints: kind === "touchEnd" ? [] : [{ x, y, id: 1, radiusX: 2, radiusY: 2, force: 1 }],
    });
    await send("touchStart", from);
    for (let i = 1; i <= steps; i += 1) {
      await send("touchMove", at(i));
      await page.waitForTimeout(16);
    }
    await send("touchEnd", to);
    await cdp.detach();
  } else {
    await page.evaluate(({ points }) => {
      const canvas = document.getElementById("openstitch");
      const touch = ([x, y]) => new Touch({ identifier: 1, target: canvas, clientX: x, clientY: y, pageX: x, pageY: y, radiusX: 2, radiusY: 2, force: 1 });
      const send = (kind, p) => canvas.dispatchEvent(new TouchEvent(kind, {
        touches: kind === "touchend" ? [] : [touch(p)], targetTouches: kind === "touchend" ? [] : [touch(p)],
        changedTouches: [touch(p)], bubbles: true, cancelable: true,
      }));
      return new Promise((done) => {
        let i = 0;
        send("touchstart", points[0]);
        const step = () => {
          i += 1;
          if (i < points.length) {
            send("touchmove", points[i]);
            requestAnimationFrame(step);
          } else {
            send("touchend", points[points.length - 1]);
            requestAnimationFrame(() => requestAnimationFrame(done));
          }
        };
        requestAnimationFrame(step);
      });
    }, { points: Array.from({ length: steps + 1 }, (_, i) => at(i)) });
  }
  await page.waitForTimeout(200);
}

// #834 and #831 in their own browser contexts: high-DPI, phone width with touch, crash
// recovery across tabs, and storage refused.
async function extraChecks(browser, type, label) {
  // #834: the editor's own shortcuts that are also the browser's (Ctrl+S Save Page, Ctrl+P
  // Print, Ctrl+O Open File) reach the editor, which keeps the browser from acting on them.
  // Ctrl+N, Ctrl+T and Ctrl+W are the browser's alone (a page never sees them in a real
  // browser), so the editor names them as taken. In its own context: Save asks the page where
  // to save, which must not hold up the other checks.
  console.log(`${label}: Ctrl+S and Ctrl+P reach the editor`);
  {
    const context = await browser.newContext({ viewport: { width: 1000, height: 700 } });
    try {
      const { page, errors } = await editorPage(context);
      await page.evaluate(() => {
        // Captured before the editor's handler; whether it prevented the browser's action is
        // read once the event has been dispatched.
        window.__events = [];
        window.addEventListener("keydown", (e) => { if (e.ctrlKey) window.__events.push(e); }, true);
        document.getElementById("openstitch").focus();
      });
      for (const key of ["s", "p"]) {
        await page.keyboard.press(`Control+${key.toUpperCase()}`);
        await page.waitForTimeout(200);
      }
      const keys = await page.evaluate(() => window.__events.map((e) => [e.key.toLowerCase(), e.defaultPrevented]));
      for (const key of ["s", "p"]) {
        if (!keys.some(([k, prevented]) => k === key && prevented)) {
          throw new Error(`Ctrl+${key.toUpperCase()} was left to the browser: ${JSON.stringify(keys)}`);
        }
      }
      const taken = (await run(page, { command: "ui.browserLimits" })).result?.browserKeys ?? [];
      for (const chord of ["Ctrl+N", "Ctrl+T", "Ctrl+W"]) {
        if (!taken.includes(chord)) throw new Error(`${chord} is not named as the browser's: ${JSON.stringify(taken)}`);
      }
      noErrors("keys", errors);
    } finally {
      await context.close();
    }
  }

  // #1260: the Point tool in the browser. A right-click on a point opens the editor's own
  // menu and the browser's stays shut (the page's contextmenu event is cancelled); the arrow
  // keys move the selected point 0.5 mm and do not scroll the page; Delete removes the point
  // and the keyboard stays with the editor; a right-click beside the points opens the
  // object's menu, whose Open is the engine's.
  console.log(`${label}: the Point tool's menus, arrow keys and Delete stay in the editor`);
  {
    const context = await browser.newContext({ viewport: { width: 1200, height: 800 } });
    try {
      const { page, errors } = await editorPage(context);
      const ok = async (request) => {
        const r = await run(page, request);
        if (!r.ok) throw new Error(`${request.command}: ${JSON.stringify(r).slice(0, 300)}`);
        return r.result;
      };
      const menu = () => ok({ command: "ui.pointMenu" });
      await ok({ command: "ui.gettingStarted", params: { cards: "dismiss" } });
      const made = await ok({ command: "shape.rectangle", params: { x: 20, y: 20, width: 40, height: 30, fill: "#2e8b57", outline: "#203040" } });
      await ok({ command: "ui.select", params: { ids: [made.id] } });
      await ok({ command: "ui.zoom", params: { fit: "page" } });
      const onScreen = async (index) => {
        await ok({ command: "ui.pointMenu", params: { points: [index] } });
        await page.waitForFunction(
          () => window.openstitch.run({ command: "ui.pointMenu" }).result?.selected?.[0]?.screen != null,
          null,
          { timeout: 10_000 },
        );
        return (await menu()).selected[0].screen;
      };
      // The opposite corner first, to know where the inside of the shape is on screen.
      const far = await onScreen(2);
      await onScreen(0);
      const start = await menu();
      if (start.pointCount !== 4 || start.open) throw new Error(`point tool: ${JSON.stringify(start).slice(0, 300)}`);
      // The hint line follows the selection (a straight corner here: To Curve, no Alt).
      if (!/To Curve/.test(start.hint ?? "") || /Alt/.test(start.hint)) throw new Error(`hint: ${start.hint}`);
      const box = await page.locator("#openstitch").boundingBox();
      const [sx, sy] = start.selected[0].screen;
      const at = [box.x + sx, box.y + sy];
      await page.evaluate(() => {
        window.__menus = [];
        window.__keys = [];
        window.addEventListener("contextmenu", (e) => window.__menus.push(e), true);
        window.addEventListener("keydown", (e) => window.__keys.push(e), true);
        document.getElementById("openstitch").focus();
      });
      const scroll = () => page.evaluate(() => [window.scrollX, window.scrollY, document.activeElement?.id ?? ""]);
      const before = await scroll();
      // Right-click on the point: our menu, with PE-DESIGN's seven entries, not the browser's.
      await page.mouse.click(at[0], at[1], { button: "right" });
      await page.waitForFunction(() => window.openstitch.run({ command: "ui.pointMenu" }).result?.open === true, null, { timeout: 10_000 });
      const shown = await menu();
      const labels = shown.entries.map((e) => `${e.label}${e.enabled ? "" : " (grey)"}`).join(", ");
      const want = "To Straight (grey), To Curve, To Smooth (grey), To Symmetry (grey), To Cusp (grey), Split at Point, Delete";
      if (labels !== want) throw new Error(`point menu: ${labels}`);
      const menus = await page.evaluate(() => window.__menus.map((e) => e.defaultPrevented));
      if (menus.length === 0 || menus.some((prevented) => !prevented)) {
        throw new Error(`the browser's own menu was not held back: ${JSON.stringify(menus)}`);
      }
      if (outDir) await page.screenshot({ path: join(outDir, `web-${label.toLowerCase()}-point-menu.png`) });
      // An entry of the menu, as a click in it runs it; the menu closes.
      const curved = await ok({ command: "ui.pointMenu", params: { action: "toCurve" } });
      if (curved.open || curved.entries[1].enabled || !curved.entries[0].enabled) {
        throw new Error(`after To Curve: ${JSON.stringify(curved.entries).slice(0, 300)}`);
      }
      // Arrow keys: 0.5 mm a press, in the editor; the page does not scroll.
      const x0 = curved.selected[0].x;
      await page.evaluate(() => document.getElementById("openstitch").focus());
      for (const key of ["ArrowRight", "ArrowRight", "ArrowDown"]) {
        await page.keyboard.press(key);
        await page.waitForTimeout(150);
      }
      await page.waitForFunction(
        (x) => Math.abs((window.openstitch.run({ command: "ui.pointMenu" }).result?.selected?.[0]?.x ?? 0) - (x + 1)) < 1e-6,
        x0,
        { timeout: 10_000 },
      );
      const moved = (await menu()).selected[0];
      if (Math.abs(moved.y - (curved.selected[0].y + 0.5)) > 1e-6) throw new Error(`ArrowDown: ${JSON.stringify(moved)}`);
      // With Shift the arrows leave the points where they are (PE-DESIGN's).
      await page.keyboard.press("Shift+ArrowRight");
      await page.waitForTimeout(300);
      if (Math.abs((await menu()).selected[0].x - moved.x) > 1e-6) throw new Error("Shift+ArrowRight moved the point");
      // Delete: the point goes, the keyboard stays with the editor.
      await page.keyboard.press("Delete");
      await page.waitForFunction(() => window.openstitch.run({ command: "ui.pointMenu" }).result?.pointCount === 3, null, { timeout: 10_000 });
      const keys = await page.evaluate(() => window.__keys.map((e) => [e.key, e.defaultPrevented]));
      // The arrow keys would scroll a page: the editor must take them from the browser. A
      // browser does nothing of its own with Delete on a canvas (the editor does not cancel
      // it, in Chromium and Firefox alike), so for Delete what counts is above and below: the
      // point is gone, the page has not moved and the keyboard is still with the editor.
      for (const key of ["ArrowRight", "ArrowDown"]) {
        if (!keys.some(([k, prevented]) => k === key && prevented)) {
          throw new Error(`${key} was left to the browser: ${JSON.stringify(keys)}`);
        }
      }
      if (!keys.some(([k]) => k === "Delete")) throw new Error(`Delete never reached the page: ${JSON.stringify(keys)}`);
      if (new URL(page.url()).pathname !== "/") throw new Error(`the page went elsewhere: ${page.url()}`);
      const after = await scroll();
      if (after[0] !== before[0] || after[1] !== before[1]) throw new Error(`the page scrolled: ${before} -> ${after}`);
      if (after[2] !== "openstitch") throw new Error(`the keyboard left the editor: focus is on "${after[2]}"`);
      // Beside the points, on the shape: the object's menu; its Open through the ribbon command.
      // Four fifths of the way to the opposite corner: inside the triangle that is left.
      const inside = [at[0] + (far[0] - sx) * 0.8, at[1] + (far[1] - sy) * 0.8];
      await page.mouse.click(inside[0], inside[1], { button: "right" });
      await page.waitForFunction(() => window.openstitch.run({ command: "ui.pointMenu" }).result?.objectMenuOpen === true, null, { timeout: 10_000 });
      const object = await menu();
      const objectLabels = object.objectMenu.map((e) => e.label).join(", ");
      if (objectLabels !== "Add Direction Line, Delete Direction Line, Reverse Direction Line, Open, Close") {
        throw new Error(`object menu: ${objectLabels}`);
      }
      if (outDir) await page.screenshot({ path: join(outDir, `web-${label.toLowerCase()}-object-menu.png`) });
      const menus2 = await page.evaluate(() => window.__menus.map((e) => e.defaultPrevented));
      if (menus2.length < 2 || menus2.some((prevented) => !prevented)) {
        throw new Error(`the browser's own menu was not held back: ${JSON.stringify(menus2)}`);
      }
      await ok({ command: "ui.run", params: { action: "openOutline" } });
      const opened = (await ok({ command: "object.list" })).find((o) => o.id === made.id);
      if (!opened || opened.region) throw new Error(`Open: ${JSON.stringify(opened).slice(0, 300)}`);
      noErrors("point tool", errors);
    } finally {
      await context.close();
    }
  }

  // High-DPI: 2 device pixels per CSS pixel; the canvas draws at full resolution (egui's
  // points times 2) and Ctrl+O still reaches the editor. Playwright's deviceScaleFactor
  // emulation leaves Chromium's and Firefox's devicePixelContentBoxSize at CSS pixels, which no
  // real screen does, so those two run at a real scale of 2 in a browser of their own
  // (Chromium's --force-device-scale-factor in its new headless mode, Firefox's devPixelsPerPx);
  // WebKit takes the emulation.
  console.log(`${label}: high-DPI (2 device pixels per CSS pixel)`);
  {
    const real = type !== webkit;
    const scaled = real
      ? await type.launch({
          timeout: 60_000,
          headless: !(type === firefox && process.env.HEADED_FIREFOX === "1"),
          ...(type === chromium ? { channel: "chromium", args: ["--force-device-scale-factor=2"] } : {}),
          ...(type === firefox ? { firefoxUserPrefs: { "webgl.force-enabled": true, "layout.css.devPixelsPerPx": "2" } } : {}),
        })
      : browser;
    const context = await scaled.newContext(real ? { viewport: null } : { viewport: { width: 1000, height: 700 }, deviceScaleFactor: 2 });
    try {
      const { page, errors } = await editorPage(context);
      const size = await page.evaluate(() => {
        const c = document.getElementById("openstitch");
        return { width: c.width, css: c.clientWidth, ratio: window.devicePixelRatio };
      });
      if (size.ratio !== 2 || Math.abs(size.width - 2 * size.css) > 2) {
        throw new Error(`high-DPI canvas is ${size.width} pixels for ${size.css} CSS pixels at ratio ${size.ratio}`);
      }
      await page.evaluate(() => document.getElementById("openstitch").focus());
      await Promise.all([page.waitForEvent("filechooser", { timeout: 15_000 }), page.keyboard.press("Control+O")]);
      if (outDir) await page.screenshot({ path: join(outDir, `web-${label.toLowerCase()}-hidpi.png`) });
      noErrors("high-DPI", errors);
    } finally {
      await context.close();
      if (real) await scaled.close().catch(() => {});
    }
  }

  // A phone (390 × 844) with touch: the editor starts with room for the canvas (the panes
  // float folded at that width, #834), a finger draws a rectangle (Chromium, Firefox) and a tap
  // selects it. That the ribbon's tabs scroll under a finger is checked natively
  // (narrow_tests).
  console.log(`${label}: phone width with touch`);
  {
    // At a real phone's 2 or 3 device pixels per CSS pixel only in WebKit: Chromium's and
    // Firefox's emulation of it gives the canvas the wrong size (see high-DPI above), which
    // misplaces every pointer and finger.
    const context = await browser.newContext({
      viewport: { width: 390, height: 844 },
      deviceScaleFactor: type === webkit ? 2 : 1,
      hasTouch: true,
      ...(type === firefox ? {} : { isMobile: true }),
    });
    try {
      const { page, errors } = await editorPage(context);
      // The start cards would cover the canvas: closed for this session.
      await run(page, { command: "ui.gettingStarted", params: { cards: "dismiss" } });
      // The middle of the screen, below the ribbon: on the canvas at this width
      // (narrow_tests::PHONE_TAP in openstitch-app checks the layout puts it there).
      const [cx, cy] = [195, 520];
      if (type !== webkit) {
        const before = await objectCount(page);
        const tool = await run(page, { command: "ui.run", params: { action: "rectangle" } });
        if (!tool.ok) throw new Error(`ui.run rectangle: ${JSON.stringify(tool).slice(0, 200)}`);
        // Once more if the first finger came while the page was still busy (seen in Firefox).
        for (let attempt = 1; ; attempt += 1) {
          await touchDrag(page, type, [cx - 60, cy - 40], [cx + 60, cy + 40]);
          const drawn = await page
            .waitForFunction((n) => window.openstitch.run({ command: "object.list" }).result?.length > n, before, { timeout: 10_000 })
            .then(() => true, () => false);
          if (drawn) break;
          if (attempt === 2) throw new Error("a finger dragged on the canvas drew no rectangle");
        }
      } else {
        const made = await run(page, { command: "shape.rectangle", params: { x: 20, y: 20, width: 60, height: 60 } });
        if (!made.ok) throw new Error(`shape.rectangle: ${JSON.stringify(made).slice(0, 200)}`);
        await run(page, { command: "ui.zoom", params: { fit: "page" } });
      }
      // A tap with the select tool inside the rectangle selects it.
      await run(page, { command: "ui.run", params: { action: "select" } });
      await run(page, { command: "ui.select", params: { mode: "none" } });
      await page.touchscreen.tap(cx, cy);
      await page.waitForFunction(() => (window.openstitch.run({ command: "ui.state" }).result?.selection ?? []).length > 0, null, { timeout: 10_000 });
      if (outDir) await page.screenshot({ path: join(outDir, `web-${label.toLowerCase()}-phone.png`) });
      noErrors("phone width", errors);
    } finally {
      await context.close();
    }
  }

  // #831: a design left unsaved in a tab that was closed comes back in the next one, from
  // IndexedDB, as an untitled design.
  console.log(`${label}: autosave, a closed tab, then recovery in a new one`);
  {
    const context = await browser.newContext({ viewport: { width: 1000, height: 700 } });
    try {
      const first = await editorPage(context);
      const made = await run(first.page, { command: "shape.rectangle", params: { x: 10, y: 10, width: 30, height: 20 } });
      if (!made.ok) throw new Error(`shape.rectangle: ${JSON.stringify(made).slice(0, 200)}`);
      // Written after 3 s without a change: the page's store is read until it holds a record.
      const stored = () => first.page.evaluate(() => new Promise((ok) => {
        const r = indexedDB.open("openstitch");
        r.onsuccess = () => {
          const q = r.result.transaction("recovery", "readonly").objectStore("recovery").count();
          q.onsuccess = () => { r.result.close(); ok(q.result); };
          q.onerror = () => ok(0);
        };
        r.onerror = () => ok(0);
      }));
      let records = 0;
      for (let i = 0; i < 40 && !records; i += 1) {
        await first.page.waitForTimeout(500);
        records = await stored();
      }
      if (!records) throw new Error("no autosave snapshot in the page's store after 20 s");
      noErrors("autosave", first.errors);
      await first.page.close();
      const second = await editorPage(context);
      const list = (await run(second.page, { command: "ui.recoveryList" })).result;
      const snapshots = list?.snapshots ?? [];
      if (snapshots.length !== 1) {
        const store = await second.page.evaluate(() => new Promise((ok) => {
          const r = indexedDB.open("openstitch");
          r.onsuccess = () => {
            const q = r.result.transaction("recovery", "readonly").objectStore("recovery").getAll();
            q.onsuccess = async () => ok({ ids: q.result.map((x) => x.id), locks: (await navigator.locks.query()).held.map((l) => l.name) });
          };
          r.onerror = () => ok(String(r.error));
        }));
        throw new Error(`recovery lists ${JSON.stringify(list).slice(0, 300)}; the store has ${JSON.stringify(store)}`);
      }
      const restored = await run(second.page, { command: "ui.recoveryRestore", params: { id: snapshots[0].id } });
      if (!restored.ok) throw new Error(`ui.recoveryRestore: ${JSON.stringify(restored).slice(0, 200)}`);
      const state = (await run(second.page, { command: "ui.state" })).result;
      if ((await objectCount(second.page)) !== 1 || state.file != null || !state.unsaved) {
        throw new Error(`after the restore: ${JSON.stringify({ file: state.file, unsaved: state.unsaved })}`);
      }
      // Restored, the closed tab's copy is gone from the store.
      const left = (await run(second.page, { command: "ui.recoveryList" })).result?.snapshots ?? [];
      if (left.length) throw new Error(`still offered after the restore: ${JSON.stringify(left)}`);
      noErrors("recovery", second.errors);
    } finally {
      await context.close();
    }
  }

  // Storage refused (a private window with storage off, blocked site data): every read and
  // write fails, the defaults apply and the editor still works.
  console.log(`${label}: browser storage refused`);
  {
    const context = await browser.newContext({ viewport: { width: 1000, height: 700 } });
    await context.addInitScript(() => {
      const refuse = () => { throw new DOMException("storage is refused", "SecurityError"); };
      for (const name of ["localStorage", "sessionStorage", "indexedDB"]) {
        Object.defineProperty(window, name, { configurable: true, get: refuse });
      }
    });
    try {
      const { page, errors } = await editorPage(context);
      const set = await run(page, { command: "ui.set", params: { name: "showStartCards", value: false } });
      if (!set.ok) throw new Error(`ui.set with storage refused: ${JSON.stringify(set).slice(0, 200)}`);
      const made = await run(page, { command: "shape.rectangle", params: { x: 10, y: 10, width: 30, height: 20 } });
      if (!made.ok) throw new Error(`shape.rectangle with storage refused: ${JSON.stringify(made).slice(0, 200)}`);
      await page.waitForTimeout(4_000);
      const list = await run(page, { command: "ui.recoveryList" });
      if (!list.ok || (list.result?.snapshots ?? []).length) throw new Error(`ui.recoveryList with storage refused: ${JSON.stringify(list).slice(0, 200)}`);
      await page.reload({ timeout: 60_000 });
      if (await started(page)) throw new Error("did not start again with storage refused");
      const prefs = await run(page, { command: "ui.settings" });
      if (prefs.result?.showStartCards?.value !== true) throw new Error("with storage refused the default should apply after a reload");
      noErrors("storage refused", errors);
    } finally {
      await context.close();
    }
  }
}

let failures = 0;
for (const [label, type] of [["Chromium", chromium], ["Firefox", firefox], ["WebKit", webkit]].filter(([l]) => !process.env.ONLY_BROWSER || process.env.ONLY_BROWSER === l)) {
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
  let shown = null;
  try {
    const page = await browser.newPage({ viewport: { width: 1100, height: 750 }, deviceScaleFactor: 1, acceptDownloads: true });
    shown = page;
    const errors = [];
    page.on("pageerror", (e) => errors.push(String(e)));
    // A Rust panic is reported on the console, and leaves the editor stopped.
    page.on("console", (m) => { if (/panicked at/.test(m.text())) errors.push(m.text().split("\n").slice(0, 2).join(" ")); });
    await page.goto(url, { timeout: 60_000 });
    console.log(`${label}: page loaded, waiting for the editor to start`);
    const message = await started(page);
    if (message && /WebGL/i.test(message) && optional.has(label.toLowerCase())) {
      console.log(`::warning::${label} has no WebGL on this runner, not tested: ${message}`);
      continue;
    }
    if (message) throw new Error(message);
    await page.waitForTimeout(500);

    // #833: the kit's samples ship with the page; File ▸ Open Sample opens one.
    console.log(`${label}: started; opening a ready-to-sew sample`);
    const sample = await run(page, { command: "ui.gettingStarted", params: { sample: "shapes" } });
    if (!sample.ok) throw new Error(`ui.gettingStarted sample: ${JSON.stringify(sample).slice(0, 200)}`);
    await page.waitForFunction(
      () => {
        const s = window.openstitch.run({ command: "ui.state" }).result;
        return s && s.loading == null && s.stats.stitches > 0;
      },
      null,
      { timeout: 30_000 },
    );
    // ...and the lettering font list says that system fonts are the desktop's.
    const limits = (await run(page, { command: "ui.browserLimits" })).result;
    if (!limits?.inBrowser || !(limits.limits ?? []).some((l) => l.key === "systemFonts")) {
      throw new Error(`ui.browserLimits has no systemFonts note: ${JSON.stringify(limits).slice(0, 200)}`);
    }
    await run(page, { command: "ui.run", params: { action: "new" } });
    // A few frames for the new design before File ▸ Open's shortcut.
    await page.waitForTimeout(500);

    console.log(`${label}: opening ${name} through the file picker (Ctrl+O)`);
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
    // #797: a photo dropped on the page becomes the background, the editor reads and decodes it
    // for the canvas, and Auto Punch converts it, all from the bytes the page holds in memory.
    console.log(`${label}: dropping a photo on the page`);
    await page.evaluate(({ b64, file }) => {
      const bytes = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
      const dt = new DataTransfer();
      dt.items.add(new File([bytes], file, { type: "image/png" }));
      const canvas = document.getElementById("openstitch");
      for (const type of ["dragenter", "dragover", "drop"]) {
        canvas.dispatchEvent(new DragEvent(type, { dataTransfer: dt, bubbles: true, cancelable: true }));
      }
    }, { b64: photoB64, file: "photo.png" });
    await page.waitForFunction(
      () => window.openstitch.run({ command: "ui.state" }).result?.background?.pixels != null,
      null,
      { timeout: 30_000 },
    );
    const background = (await run(page, { command: "ui.state" })).result.background;
    const punched = await run(page, {
      command: "digitize.autoPunch",
      params: { path: background.path, width: 60, colors: 4 },
    });
    if (!punched.ok || !(punched.result?.ids?.length > 0)) {
      throw new Error(`Auto Punch of the dropped photo: ${JSON.stringify(punched).slice(0, 200)}`);
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
    // #829: a thread chart the user adds is still there after a reload.
    console.log(`${label}: adding a thread chart, then a reload`);
    const made = await run(page, { command: "threads.createChart", params: { name: "Page test shelf" } });
    if (!made.ok) throw new Error(`threads.createChart: ${JSON.stringify(made).slice(0, 200)}`);
    await page.reload({ timeout: 60_000 });
    const back = await started(page);
    if (back) throw new Error(`after a reload: ${back}`);
    const charts = await run(page, { command: "threads.charts" });
    if (!(charts.result ?? []).some((c) => c.name === "Page test shelf")) {
      throw new Error(`the thread chart is gone after a reload: ${JSON.stringify(charts).slice(0, 200)}`);
    }
    if (outDir) {
      mkdirSync(outDir, { recursive: true });
      await page.screenshot({ path: join(outDir, `web-${label.toLowerCase()}.png`) });
    }
    if (errors.length) throw new Error(`page errors: ${errors.join("; ")}`);
    await extraChecks(browser, type, label);
    console.log(`ok ${label}: started, opened ${name} with the file picker, the PES and DST downloads are identical to the desktop build, a dropped photo shows and converts, preferences and thread charts last across reloads, a sample opens, Ctrl+S and Ctrl+P reach the editor, it runs at 2× pixels and at phone width with touch, a closed tab's design is recovered, and it runs with storage refused`);
  } catch (e) {
    console.error(`FAIL ${label}: ${e.message}`);
    failures += 1;
    // What the page showed when it failed, beside the other screenshots.
    if (outDir && shown) {
      mkdirSync(outDir, { recursive: true });
      await shown.screenshot({ path: join(outDir, `web-${label.toLowerCase()}-failed.png`) }).catch(() => {});
    }
  } finally {
    await browser.close().catch(() => {});
  }
}
server.closeAllConnections?.();
server.close();
if (failures) process.exit(1);
console.log("web page test passed");
process.exit(0);
