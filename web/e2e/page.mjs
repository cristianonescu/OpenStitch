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
import { inflateSync } from "node:zlib";
import { mkdirSync, readFileSync } from "node:fs";
import { basename, dirname, extname, join, resolve, posix } from "node:path";
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
  // A URL path, so with forward slashes on every system (Windows: the page test runs there too).
  const path = posix.normalize(decodeURIComponent(new URL(req.url, "http://x").pathname));
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


// A PNG as Playwright's screenshots are (8 bits a channel, RGB or RGBA, not interlaced):
// its size and a reader of one pixel's [r, g, b].
const png = (buffer) => {
  const width = buffer.readUInt32BE(16);
  const height = buffer.readUInt32BE(20);
  const channels = buffer[25] === 6 ? 4 : 3;
  if (buffer[24] !== 8 || (buffer[25] !== 6 && buffer[25] !== 2) || buffer[28] !== 0) throw new Error("a PNG of a kind this test does not read");
  const parts = [];
  for (let at = 8; at < buffer.length; ) {
    const length = buffer.readUInt32BE(at);
    if (buffer.toString("latin1", at + 4, at + 8) === "IDAT") parts.push(buffer.subarray(at + 8, at + 8 + length));
    at += 12 + length;
  }
  const raw = inflateSync(Buffer.concat(parts));
  const stride = width * channels;
  const rows = Buffer.alloc(stride * height);
  for (let y = 0; y < height; y += 1) {
    const filter = raw[y * (stride + 1)];
    for (let x = 0; x < stride; x += 1) {
      const value = raw[y * (stride + 1) + 1 + x];
      const left = x >= channels ? rows[y * stride + x - channels] : 0;
      const up = y > 0 ? rows[(y - 1) * stride + x] : 0;
      const upLeft = y > 0 && x >= channels ? rows[(y - 1) * stride + x - channels] : 0;
      let predicted = 0;
      if (filter === 1) predicted = left;
      else if (filter === 2) predicted = up;
      else if (filter === 3) predicted = (left + up) >> 1;
      else if (filter === 4) {
        const p = left + up - upLeft;
        const [a, b, c] = [Math.abs(p - left), Math.abs(p - up), Math.abs(p - upLeft)];
        predicted = a <= b && a <= c ? left : b <= c ? up : upLeft;
      }
      rows[y * stride + x] = (value + predicted) & 255;
    }
  }
  return { width, height, pixel: (x, y) => [rows[y * stride + x * channels], rows[y * stride + x * channels + 1], rows[y * stride + x * channels + 2]] };
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

  // #1260: the Point tool in the browser. One click on the outline inserts a point, a mouse
  // drag moves a point and Ctrl+click adds one to the selection, none of them the browser's
  // to act on. A right-click on a point opens the editor's own
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
      // Waits on the editor's own state (ui.pointMenu), never on a fixed sleep: a browser on a
      // slow software renderer draws few frames a second. A wait that runs out says which step
      // it was and what the editor reported then.
      const until = async (step, test, arg = null) => {
        try {
          await page.waitForFunction(test, arg, { timeout: 30_000 });
        } catch (e) {
          const seen = await page
            .evaluate(() => JSON.stringify(window.openstitch.run({ command: "ui.pointMenu" }).result ?? null))
            .catch(() => "?");
          throw new Error(`point tool, ${step}: ${String(e.message).split("\n")[0]}; ui.pointMenu said ${seen.slice(0, 500)}`);
        }
      };
      const pointX = () => window.openstitch.run({ command: "ui.pointMenu" }).result?.selected?.[0]?.x ?? null;
      // A menu that reports open has to be ON SCREEN before it is photographed: ui.pointMenu
      // gives the rectangle its panel was painted in (null while it is only measured) and the
      // panel's colour; a screenshot of that rectangle must then be mostly that colour (the
      // canvas under it is the page, its grid and the shape's stitches). Polled, because the
      // browser shows a frame a moment after the editor has drawn it.
      const painted = async (step) => {
        await until(`${step}: the menu is drawn`, () => window.openstitch.run({ command: "ui.pointMenu" }).result?.menuRect != null);
        const { menuRect: r, menuFill } = await menu();
        const fill = [1, 3, 5].map((i) => parseInt(menuFill.slice(i, i + 2), 16));
        const clip = { x: Math.round(box.x + r[0]) + 2, y: Math.round(box.y + r[1]) + 2, width: Math.round(r[2]) - 4, height: Math.round(r[3]) - 4 };
        let share = 0;
        for (let attempt = 0; attempt < 40; attempt += 1) {
            const { width, height, pixel } = png(await page.screenshot({ clip }));
            let same = 0;
            let all = 0;
            for (let y = 0; y < height; y += 3) {
              for (let x = 0; x < width; x += 3) {
                const p = pixel(x, y);
                all += 1;
                if (Math.abs(p[0] - fill[0]) <= 8 && Math.abs(p[1] - fill[1]) <= 8 && Math.abs(p[2] - fill[2]) <= 8) same += 1;
              }
            }
            share = same / all;
            if (share >= 0.6) return;
            await page.waitForTimeout(250);
        }
        throw new Error(`point tool, ${step}: the menu reports it was drawn at ${JSON.stringify(r)} but only ${Math.round(share * 100)} % of that rectangle has its panel colour ${menuFill}: it is not on screen`);
      };
      await ok({ command: "ui.gettingStarted", params: { cards: "dismiss" } });
      const made = await ok({ command: "shape.rectangle", params: { x: 20, y: 20, width: 40, height: 30, fill: "#2e8b57", outline: "#203040" } });
      await ok({ command: "ui.select", params: { ids: [made.id] } });
      await ok({ command: "ui.zoom", params: { fit: "page" } });
      const onScreen = async (index) => {
        await ok({ command: "ui.pointMenu", params: { points: [index] } });
        await until(
          `point ${index} selected and placed on screen`,
          (i) => {
            const p = window.openstitch.run({ command: "ui.pointMenu" }).result?.selected?.[0];
            return p?.index === i && p.screen != null;
          },
          index,
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
      // One click on the outline inserts a point there (the middle of the top side).
      await page.mouse.click(box.x + (sx + far[0]) / 2, box.y + sy);
      await until("one click on the outline inserts a point (5 points)", () => window.openstitch.run({ command: "ui.pointMenu" }).result?.pointCount === 5);
      await ok({ command: "history.undo" });
      // Dragging a point with the mouse moves it; the page neither scrolls nor selects text.
      await onScreen(0);
      await page.mouse.move(at[0], at[1]);
      await page.mouse.down();
      for (let step = 1; step <= 5; step += 1) await page.mouse.move(at[0] - 5 * step, at[1] - 4 * step);
      await page.mouse.up();
      await until(
        `the dragged point is at ${sx - 25}, ${sy - 20} on screen (within 6)`,
        ([x, y]) => {
          const s = window.openstitch.run({ command: "ui.pointMenu" }).result?.selected?.[0]?.screen;
          return s && Math.abs(s[0] - x) < 6 && Math.abs(s[1] - y) < 6;
        },
        [sx - 25, sy - 20],
      );
      const afterDrag = await scroll();
      const picked = await page.evaluate(() => String(window.getSelection?.() ?? ""));
      if (afterDrag[0] !== before[0] || afterDrag[1] !== before[1] || picked !== "") {
        throw new Error(`a point drag moved the page or selected text: ${before} -> ${afterDrag}, "${picked}"`);
      }
      await ok({ command: "history.undo" });
      // Ctrl+click on a second point adds it to the selection; the browser opens no menu and
      // no tab of its own.
      await onScreen(0);
      // The key is held down around the click: page.mouse.click takes no modifiers.
      const clickWith = async (key, x, y) => {
        await page.keyboard.down(key);
        await page.mouse.click(x, y);
        await page.keyboard.up(key);
      };
      await clickWith("Control", box.x + far[0], box.y + far[1]);
      await until("Ctrl+click adds a second point (2 selected)", () => window.openstitch.run({ command: "ui.pointMenu" }).result?.selected?.length === 2);
      const stray = await page.evaluate(() => window.__menus.length);
      if (stray !== 0 || context.pages().length !== 1) {
        throw new Error(`Ctrl+click was the browser's: ${stray} menu(s), ${context.pages().length} page(s)`);
      }
      // Shift+click adds a point too (PE-DESIGN's, the owner's hand check of 2026-11-19);
      // the browser selects no text and opens nothing.
      const third = await onScreen(3);
      await onScreen(0);
      await clickWith("Control", box.x + far[0], box.y + far[1]);
      await until("Ctrl+click adds the second point again (2 selected)", () => window.openstitch.run({ command: "ui.pointMenu" }).result?.selected?.length === 2);
      await clickWith("Shift", box.x + third[0], box.y + third[1]);
      await until("Shift+click adds a third point (3 selected)", () => window.openstitch.run({ command: "ui.pointMenu" }).result?.selected?.length === 3);
      // Ctrl+click on a selected point takes it out; a plain click on a point that is not
      // selected selects that one alone.
      await clickWith("Control", box.x + third[0], box.y + third[1]);
      await until("Ctrl+click on a selected point takes it out (2 selected)", () => window.openstitch.run({ command: "ui.pointMenu" }).result?.selected?.length === 2);
      await page.mouse.click(box.x + third[0], box.y + third[1]);
      await until("a plain click on another point selects it alone", (i) => {
        const s = window.openstitch.run({ command: "ui.pointMenu" }).result?.selected;
        return s?.length === 1 && s[0].index === i;
      }, 3);
      const shiftPicked = await page.evaluate(() => String(window.getSelection?.() ?? ""));
      if (shiftPicked !== "" || context.pages().length !== 1) {
        throw new Error(`Shift+click was the browser's: "${shiftPicked}", ${context.pages().length} page(s)`);
      }
      await onScreen(0);
      // Right-click on the point: our menu, with PE-DESIGN's seven entries, not the browser's.
      await page.mouse.click(at[0], at[1], { button: "right" });
      await until("a right-click on the point opens the point menu", () => window.openstitch.run({ command: "ui.pointMenu" }).result?.open === true);
      const shown = await menu();
      const labels = shown.entries.map((e) => `${e.label}${e.enabled ? "" : " (grey)"}`).join(", ");
      const want = "To Straight (grey), To Curve, To Smooth (grey), To Symmetry (grey), To Cusp (grey), Split at Point, Delete";
      if (labels !== want) throw new Error(`point menu: ${labels}`);
      const menus = await page.evaluate(() => window.__menus.map((e) => e.defaultPrevented));
      if (menus.length === 0 || menus.some((prevented) => !prevented)) {
        throw new Error(`the browser's own menu was not held back: ${JSON.stringify(menus)}`);
      }
      await painted("the point menu");
      if (outDir) await page.screenshot({ path: join(outDir, `web-${label.toLowerCase()}-point-menu.png`) });
      // An entry of the menu, as a click in it runs it; the menu closes.
      const curved = await ok({ command: "ui.pointMenu", params: { action: "toCurve" } });
      if (curved.open || curved.entries[1].enabled || !curved.entries[0].enabled) {
        throw new Error(`after To Curve: ${JSON.stringify(curved.entries).slice(0, 300)}`);
      }
      // Arrow keys: 0.5 mm a press, in the editor; the page does not scroll. One key at a
      // time, each waited for: two presses that reach the editor within one drawn frame move
      // the point once, as two quick presses of any of its shortcuts do.
      const x0 = curved.selected[0].x;
      const y0 = curved.selected[0].y;
      await page.evaluate(() => document.getElementById("openstitch").focus());
      const near = (want) => `Math.abs((${pointX.toString()})() - ${want}) < 1e-6`;
      await page.keyboard.press("ArrowRight");
      await until("ArrowRight moves the point 0.5 mm", near(x0 + 0.5));
      await page.keyboard.press("ArrowRight");
      await until("a second ArrowRight moves it 0.5 mm more", near(x0 + 1));
      await page.keyboard.press("ArrowDown");
      await until(
        "ArrowDown moves the point 0.5 mm down",
        (y) => Math.abs((window.openstitch.run({ command: "ui.pointMenu" }).result?.selected?.[0]?.y ?? 0) - y) < 1e-6,
        y0 + 0.5,
      );
      const moved = (await menu()).selected[0];
      // With Shift or Ctrl the arrows move no point and change the selection instead
      // (PE-DESIGN capture f): Shift+arrow grows it by one point and shrinks it again,
      // Ctrl+arrow moves it to the neighbouring point. The browser must not take them.
      const pickedNow = () => window.openstitch.run({ command: "ui.pointMenu" }).result?.selected?.map((p) => p.index).sort().join(",");
      const untilPicked = (step, want) => until(step, `(${pickedNow.toString()})() === "${want}"`);
      const i0 = moved.index;
      const count = (await menu()).pointCount;
      const next = (i0 + 1) % count;
      await page.keyboard.press("Shift+ArrowRight");
      await untilPicked("Shift+ArrowRight adds the next point", [i0, next].sort().join(","));
      await page.keyboard.press("Shift+ArrowLeft");
      await untilPicked("Shift+ArrowLeft shrinks the selection back", `${i0}`);
      await page.keyboard.press("Control+ArrowRight");
      await untilPicked("Ctrl+ArrowRight selects the next point", `${next}`);
      await page.keyboard.press("Control+ArrowLeft");
      await untilPicked("Ctrl+ArrowLeft selects the point before it", `${i0}`);
      const still = (await menu()).selected[0];
      if (Math.abs(still.x - moved.x) > 1e-6 || Math.abs(still.y - moved.y) > 1e-6) {
        throw new Error(`Shift+arrow or Ctrl+arrow moved the point: ${JSON.stringify([moved, still])}`);
      }
      await page.keyboard.press("ArrowLeft");
      await until("ArrowLeft moves the point 0.5 mm back", near(moved.x - 0.5));
      // Delete: the point goes, the keyboard stays with the editor.
      await page.keyboard.press("Delete");
      await until("Delete removes the point (3 points)", () => window.openstitch.run({ command: "ui.pointMenu" }).result?.pointCount === 3);
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
      await until("a right-click on the shape opens the object menu", () => window.openstitch.run({ command: "ui.pointMenu" }).result?.objectMenuOpen === true);
      const object = await menu();
      const objectLabels = object.objectMenu.map((e) => e.label).join(", ");
      if (objectLabels !== "Add Direction Line, Delete Direction Line, Reverse Direction Line, Open, Close") {
        throw new Error(`object menu: ${objectLabels}`);
      }
      await painted("the object menu");
      if (outDir) await page.screenshot({ path: join(outDir, `web-${label.toLowerCase()}-object-menu.png`) });
      const menus2 = await page.evaluate(() => window.__menus.map((e) => e.defaultPrevented));
      if (menus2.length < 2 || menus2.some((prevented) => !prevented)) {
        throw new Error(`the browser's own menu was not held back: ${JSON.stringify(menus2)}`);
      }
      await ok({ command: "ui.run", params: { action: "openOutline" } });
      const opened = (await ok({ command: "object.list" })).find((o) => o.id === made.id);
      if (!opened || opened.region) throw new Error(`Open: ${JSON.stringify(opened).slice(0, 300)}`);
      // The direction line of a filled shape (PE-DESIGN capture g): a click on its start
      // square selects it (the square turns black on the page); Delete then removes nothing.
      const filled = await ok({ command: "shape.rectangle", params: { x: 62, y: 62, width: 20, height: 14, fill: "#2e8b57", outline: "#203040" } });
      await ok({ command: "object.setRegion", params: { id: filled.id, angle: 45 } });
      await ok({ command: "ui.select", params: { ids: [filled.id] } });
      // Open was run through ui.run in the step before: that closes the object menu, as a
      // click on the entry does (#1271), so the next click cannot land on one of its rows.
      const afterRun = await menu();
      if (afterRun.objectMenuOpen !== false || afterRun.open !== false) {
        throw new Error(`ui.run openOutline left a menu open: ${JSON.stringify({ open: afterRun.open, objectMenuOpen: afterRun.objectMenuOpen })}`);
      }
      await until("the filled shape shows its direction line", () => window.openstitch.run({ command: "ui.pointMenu" }).result?.directionLine?.screenFrom != null);
      const shownLine = (await menu()).directionLine;
      if (shownLine.angle !== 45 || shownLine.selected) throw new Error(`the direction line before the click: ${JSON.stringify(shownLine)}`);
      const square = shownLine.screenFrom;
      const centre = async () => png(await page.screenshot()).pixel(Math.round(box.x + square[0]), Math.round(box.y + square[1]));
      const empty = await centre();
      if (empty[0] < 200 || empty[1] < 200 || empty[2] < 200) throw new Error(`the unselected square is not white inside: ${empty}`);
      await page.mouse.click(box.x + square[0], box.y + square[1]);
      await until("a click on the square selects the direction line", () => window.openstitch.run({ command: "ui.pointMenu" }).result?.directionLine?.selected === true);
      let black = [];
      for (let attempt = 0; attempt < 40; attempt += 1) {
        black = await centre();
        if (black[0] < 80 && black[1] < 80 && black[2] < 80) break;
        await page.waitForTimeout(250);
      }
      if (!(black[0] < 80 && black[1] < 80 && black[2] < 80)) throw new Error(`the selected square is not black on the page: ${black}`);
      if (outDir) await page.screenshot({ path: join(outDir, `web-${label.toLowerCase()}-direction-line.png`) });
      await page.keyboard.press("Delete");
      await page.waitForTimeout(300);
      if (!(await ok({ command: "object.list" })).some((o) => o.id === filled.id)) throw new Error("Delete with the direction line selected removed the shape");
      const kept = (await menu()).directionLine;
      if (kept?.angle !== 45 || kept.selected !== true) throw new Error(`the direction line after the click and Delete: ${JSON.stringify(kept)}`);
      // Add Direction Line in the browser (PE-DESIGN's two clicks, #1273): the two clicks
      // outside the shape add a line; a click on its square selects it; Delete removes it.
      const whole = (await menu()).directionLine;
      const scale = (whole.screenTo[0] - whole.screenFrom[0]) / (whole.to[0] - whole.from[0]);
      const onPage = ([x, y]) => [box.x + whole.screenFrom[0] + (x - whole.from[0]) * scale, box.y + whole.screenFrom[1] + (y - whole.from[1]) * scale];
      await ok({ command: "ui.run", params: { action: "addDirection" } });
      await until("Add Direction Line waits for its two clicks", () => window.openstitch.run({ command: "ui.pointMenu" }).result?.addingDirectionLine === true);
      await page.mouse.click(...onPage([58, 68]));
      await page.mouse.click(...onPage([70, 58]));
      await until("the two clicks add a second direction line", () => window.openstitch.run({ command: "ui.pointMenu" }).result?.directionLines?.length === 2);
      const added = (await menu()).directionLines;
      if (added.some((l) => !l.stored || l.selected) || (await menu()).addingDirectionLine) throw new Error(`after Add Direction Line: ${JSON.stringify(added).slice(0, 400)}`);
      const fresh = added[added.length - 1];
      await page.mouse.click(box.x + fresh.screenFrom[0], box.y + fresh.screenFrom[1]);
      await until("a click on the new line's square selects that line", () => window.openstitch.run({ command: "ui.pointMenu" }).result?.directionLines?.[1]?.selected === true);
      await page.keyboard.press("Delete");
      await until("Delete removes the selected direction line", () => {
        const s = window.openstitch.run({ command: "ui.pointMenu" }).result;
        return s?.directionLines?.length === 1 && s.directionLines[0].stored === false;
      });
      if (!(await ok({ command: "object.list" })).some((o) => o.id === filled.id)) throw new Error("Delete with a direction line selected removed the shape");
      // Alt + drag of a line's end onto another open line's end joins the two lines; Alt is
      // not the browser's here (no menu bar opens, the page stays).
      const lineA = await ok({ command: "shape.polygon", params: { points: [[10, 64], [30, 64], [30, 56]], closed: false, outline: "#203040" } });
      const lineB = await ok({ command: "shape.polygon", params: { points: [[45, 56], [60, 56], [60, 64]], closed: false, outline: "#203040" } });
      // Every open line of the document has connectable ends: the two new lines, and the
      // first shape, which Open made an open line above. Its ends are read from the editor.
      await ok({ command: "ui.select", params: { ids: [made.id] } });
      const madeLast = (await menu()).pointCount - 1;
      const endOf = async (index) => {
        await onScreen(index);
        const p = (await menu()).selected[0];
        return { id: made.id, index, x: p.x, y: p.y };
      };
      const expected = [
        { id: lineA.id, index: 0, x: 10, y: 64 },
        await endOf(0),
        await endOf(madeLast),
        { id: lineB.id, index: 0, x: 45, y: 56 },
        { id: lineB.id, index: 2, x: 60, y: 64 },
      ];
      const sameEnds = (list, want) => {
        const key = (e) => `${e.id}:${e.index}:${e.x.toFixed(2)}:${e.y.toFixed(2)}`;
        return JSON.stringify(list.map(key).sort()) === JSON.stringify(want.map(key).sort());
      };
      await ok({ command: "ui.select", params: { ids: [lineA.id] } });
      const endAt = await onScreen(2);
      const offered = (await menu()).connectable;
      if (!sameEnds(offered, expected)) throw new Error(`connectable: ${JSON.stringify(offered).slice(0, 400)}, expected ${JSON.stringify(expected)}`);
      const target = offered.find((e) => e.id === lineB.id && e.index === 0);
      if (!target?.screen) throw new Error(`connectable: ${JSON.stringify(offered).slice(0, 300)}`);
      await page.keyboard.down("Alt");
      await page.mouse.move(box.x + endAt[0], box.y + endAt[1]);
      await page.mouse.down();
      await page.mouse.move(box.x + target.screen[0], box.y + target.screen[1], { steps: 8 });
      // While the drag is held on the other end: the red square is painted there and an x on
      // each other connectable end (pixels of the page, as the menu check reads them).
      await until("the Alt-dragged end lies on the other line's end", () => window.openstitch.run({ command: "ui.pointMenu" }).result?.connectOverlap != null);
      const others = (await menu()).connectable.filter((e) => !(e.id === target.id && e.index === target.index));
      const expectedOthers = expected.filter((e) => !(e.id === target.id && e.index === target.index));
      if (!sameEnds(others, expectedOthers)) throw new Error(`connectable during the drag: ${JSON.stringify(others).slice(0, 400)}, expected ${JSON.stringify(expectedOthers)}`);
      const red = (p) => p[0] > 170 && p[1] < 100 && p[2] < 100;
      const dark = (p) => p[0] < 110 && p[1] < 110 && p[2] < 110;
      let marks = "";
      for (let attempt = 0; attempt < 40 && marks !== "ok"; attempt += 1) {
        const shot = png(await page.screenshot());
        const near = (x, y, test) => {
          for (let dy = -1; dy <= 1; dy += 1) {
            for (let dx = -1; dx <= 1; dx += 1) {
              if (test(shot.pixel(Math.round(x) + dx, Math.round(y) + dy))) return true;
            }
          }
          return false;
        };
        const count = (at, offsets, test) => offsets.filter(([dx, dy]) => near(box.x + at[0] + dx, box.y + at[1] + dy, test)).length;
        const square = count(target.screen, [[6.5, 0], [-6.5, 0], [0, 6.5], [0, -6.5]], red);
        const crosses = others.map((e) => count(e.screen, [[4, 4], [4, -4], [-4, 4], [-4, -4]], dark));
        marks = square >= 3 && crosses.every((n) => n >= 3) ? "ok" : `red sides ${square} of 4, x arms ${JSON.stringify(crosses)} of 4 each`;
        if (marks !== "ok") await page.waitForTimeout(250);
      }
      if (marks !== "ok") throw new Error(`point tool, the Alt-connect marks are not on screen: ${marks}`);
      if (outDir) await page.screenshot({ path: join(outDir, `web-${label.toLowerCase()}-alt-connect.png`) });
      await page.mouse.up();
      await page.keyboard.up("Alt");
      await until("Alt + drag onto the other line's end joins the two lines (5 points show)", () => window.openstitch.run({ command: "ui.pointMenu" }).result?.pointCount === 5);
      const lines = (await ok({ command: "object.list" })).filter((o) => o.id === lineA.id || o.id === lineB.id);
      if (lines.length !== 1 || lines[0].id !== lineA.id) throw new Error(`Alt + drag: ${JSON.stringify(lines).slice(0, 300)}`);
      if (new URL(page.url()).pathname !== "/") throw new Error(`the page went elsewhere: ${page.url()}`);
      // #1272: the Point tool on a stitch pattern. Two running lines in two colours are made
      // into one stitch pattern (object.toStitches of a group), and split at a point of the
      // second colour so that there are two patterns to connect.
      const redLine = await ok({ command: "shape.polygon", params: { points: [[20, 70], [40, 70]], closed: false, outline: "#d32f2f" } });
      const blueLine = await ok({ command: "shape.polygon", params: { points: [[20, 80], [40, 80]], closed: false, outline: "#1f3a93" } });
      for (const id of [redLine.id, blueLine.id]) await ok({ command: "object.setLine", params: { id, type: "running", pitch: 2.0 } });
      await ok({ command: "object.group", params: { ids: [redLine.id, blueLine.id] } });
      const converted = await ok({ command: "object.toStitches", params: { ids: [redLine.id] } });
      const patterns = (await ok({ command: "object.list" })).filter((o) => o.kind === "stitches");
      if (patterns.length < 1) throw new Error(`no stitch pattern: ${JSON.stringify(converted).slice(0, 300)}`);
      const pattern = patterns[patterns.length - 1];
      await ok({ command: "ui.select", params: { ids: [pattern.id] } });
      await page.evaluate((id) => { window.__pattern = id; }, pattern.id);
      const stitch = (params) => ok({ command: "ui.stitchPoints", params });
      const st = await stitch({ end: "first" });
      if (!st.selectedAt?.[0]?.screen) throw new Error(`ui.stitchPoints: ${JSON.stringify(st).slice(0, 300)}`);
      // One click on a stitch inserts a needle point: the middle of a 2 mm stitch in the middle
      // of the line, away from the lock stitches' points 0.3 mm apart.
      const points = await ok({ command: "stitches.points", params: { id: pattern.id } });
      const pts = points.points.filter((p) => p.command === "stitch");
      const long = (i) => i > 0 && Math.abs(pts[i].x - pts[i - 1].x) > 1.5 && Math.abs(pts[i].y - pts[i - 1].y) < 0.01;
      const runs = pts.map((_, i) => i).filter((i) => long(i) && long(i - 1) && i + 1 < pts.length && long(i + 1));
      const k = runs.length ? runs[Math.floor(runs.length / 2)] : -1;
      if (k < 1) throw new Error(`no 2 mm stitch: ${JSON.stringify(pts.slice(0, 8))}`);
      // Zoomed in on that stitch, as one would to click between points 2 mm apart.
      await ok({ command: "ui.zoom", params: { rect: [pts[k - 1].x - 4, pts[k - 1].y - 4, 10, 8] } });
      await stitch({ select: [pts[k - 1].index] });
      const a1 = (await stitch({})).selectedAt[0].screen;
      await stitch({ select: [pts[k].index] });
      const a2 = (await stitch({})).selectedAt[0].screen;
      const entries = async () => (await ok({ command: "stitches.points", params: { id: pattern.id } })).points.length;
      const n0 = await entries();
      await page.mouse.click(box.x + (a1[0] + a2[0]) / 2, box.y + (a1[1] + a2[1]) / 2);
      await until("one click on a stitch inserts a needle point", (n) => window.openstitch.run({ command: "stitches.points", params: { id: window.__pattern } }).result?.points?.length === n + 1, n0).catch(async () => {
        const after = await stitch({});
        throw new Error(`stitch pattern click-insert: ${n0} -> ${await entries()}; clicked ${JSON.stringify([a1, a2])}, then ${JSON.stringify({ selected: after.selected, selectedAt: after.selectedAt, hint: after.hint, tool: after.note })}`);
      });
      await ok({ command: "history.undo" });
      // Ctrl+click adds a needle point to the selection.
      await stitch({ select: [pts[k - 1].index] });
      await clickWith("Control", box.x + a2[0], box.y + a2[1]);
      await until("Ctrl+click adds a needle point", () => (window.openstitch.run({ command: "ui.stitchPoints" }).result?.selected?.length ?? 0) === 2);
      // Tab selects every needle point of the selected point's colour block (PE-DESIGN capture
      // k6), and stays with the editor; Tab again selects none.
      const held = pts[k - 1].index;
      await page.keyboard.press("Tab");
      const tab = await stitch({});
      const every = tab.colors.find((c) => c.entry <= held && held <= c.exit)?.points;
      if (tab.selected.length !== every) throw new Error(`Tab: ${JSON.stringify(tab).slice(0, 300)}`);
      await page.keyboard.press("Tab");
      await until("Tab again selects none", () => (window.openstitch.run({ command: "ui.stitchPoints" }).result?.selected?.length ?? -1) === 0);
      // Alt + drag of the exit point onto another pattern's entry point connects the two, with
      // the whole page in view again.
      await ok({ command: "ui.zoom", params: { fit: "page" } });
      const lastColour = tab.colors[tab.colors.length - 1];
      const cutAt = (await ok({ command: "stitches.points", params: { id: pattern.id } })).points.filter((p) => p.color === tab.colors.length - 1 && p.command === "stitch")[4].index;
      const split = await ok({ command: "stitches.splitAt", params: { id: pattern.id, index: cutAt } });
      const part = split.ids[1];
      await ok({ command: "ui.select", params: { ids: [pattern.id] } });
      const ex = await stitch({ end: "last" });
      // Every other stitch pattern's entry point is offered, the part split off among them.
      const partEntry = ex.connectable?.find((c) => c.id === part);
      if (!partEntry?.screen) throw new Error(`connectable: ${JSON.stringify(ex).slice(0, 400)} (${lastColour.points}, part ${part})`);
      const stitchPatterns = async () => (await ok({ command: "object.list" })).filter((o) => o.kind === "stitches").length;
      const patternsBefore = await stitchPatterns();
      const exitAt = ex.selectedAt[0].screen;
      const onto = partEntry.screen;
      await page.keyboard.down("Alt");
      await page.mouse.move(box.x + exitAt[0], box.y + exitAt[1]);
      await page.mouse.down();
      await page.mouse.move(box.x + onto[0], box.y + onto[1], { steps: 8 });
      await until("the Alt-dragged exit point lies on the other pattern's entry", () => window.openstitch.run({ command: "ui.stitchPoints" }).result?.connectOverlap != null);
      await page.mouse.up();
      await page.keyboard.up("Alt");
      await until("Alt + drag connects the two stitch patterns", (n) => (window.openstitch.run({ command: "object.list" }).result ?? []).filter((o) => o.kind === "stitches").length === n - 1, patternsBefore);
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
    await resizeChecks(browser, type, label);
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

// #1266: a browser window made smaller and larger again keeps the page in view. With the view
// as the editor fitted it, the page fits again; with a view the user zoomed, the zoom and the
// design point in the middle of the canvas are kept; and from any of the sizes Fit (ui.zoom)
// brings the page back. The sizes: a docked layout squeezed low, a phone-narrow window, and one
// that leaves the canvas no room. In Chromium the same with the page zoomed by the browser
// (150 %: fewer CSS pixels, more device pixels each, through the DevTools protocol; Firefox and
// WebKit have no way to zoom a page from a test). Screenshots before and after go to OUT_DIR.
// (A function declaration: the loop above calls it.)
async function resizeChecks(browser, type, label) {
  console.log(`${label}: the window made smaller and larger again keeps the page in view`);
  const big = { width: 1100, height: 750 };
  const small = [{ width: 700, height: 260 }, { width: 320, height: 240 }, { width: 150, height: 100 }];
  const context = await browser.newContext({ viewport: big, deviceScaleFactor: 1 });
  try {
    const { page, errors } = await editorPage(context);
    // The view once the editor has drawn the same one twice in a row.
    const view = async () => {
      let last = "";
      for (let i = 0; i < 40; i += 1) {
        await page.waitForTimeout(150);
        const s = (await run(page, { command: "ui.state" })).result;
        const now = JSON.stringify({ zoom: s?.zoomPercent, ...s?.view });
        if (s?.view && now === last) return JSON.parse(now);
        last = now;
      }
      throw new Error(`the view did not settle: ${last}`);
    };
    const shot = async (name) => {
      if (!outDir) return;
      mkdirSync(outDir, { recursive: true });
      await page.screenshot({ path: join(outDir, `web-${label.toLowerCase()}-resize-${name}.png`) });
    };
    const usable = (v) => v.canvasPoints[0] >= 32 && v.canvasPoints[1] >= 32;
    const inView = (what, v) => {
      if (!(v.zoom > 0) || !Number.isFinite(v.zoom) || !v.centerMm.every(Number.isFinite)) {
        throw new Error(`${what}: the view is broken: ${JSON.stringify(v)}`);
      }
      if (usable(v) && !(v.pagePointsInCanvas[0] > 0 && v.pagePointsInCanvas[1] > 0)) {
        throw new Error(`${what}: the page is outside the canvas: ${JSON.stringify(v)}`);
      }
    };
    const sameCanvas = (a, b) => a.canvasPoints[0] === b.canvasPoints[0] && a.canvasPoints[1] === b.canvasPoints[1];
    const near = (a, b, by) => Math.abs(a - b) <= by;
    const sameCentre = (a, b) => near(a.centerMm[0], b.centerMm[0], 0.05) && near(a.centerMm[1], b.centerMm[1], 0.05);
    // Down through the sizes and back up, with `resize` setting the window's size.
    const round = async (what, resize, zoomed) => {
      const before = await view();
      inView(`${what}, before`, before);
      for (const size of small) {
        await resize(size);
        const at = await view();
        inView(`${what}, at ${size.width}x${size.height}`, at);
        if (zoomed && !near(at.zoom, before.zoom, 0.01)) {
          throw new Error(`${what}, at ${size.width}x${size.height}: the zoom went from ${before.zoom} to ${at.zoom}`);
        }
        if (zoomed && usable(at) && !sameCentre(at, before)) {
          throw new Error(`${what}, at ${size.width}x${size.height}: the canvas's middle shows ${at.centerMm}, it showed ${before.centerMm}`);
        }
      }
      await resize(big);
      const after = await view();
      inView(`${what}, after`, after);
      // The narrow layout folds the panes away and they stay folded, so the canvas can be
      // wider than before: a fitted page then fits that; a zoomed view keeps zoom and middle.
      if (!sameCentre(after, before) || ((zoomed || sameCanvas(after, before)) && !near(after.zoom, before.zoom, 0.01))) {
        throw new Error(`${what}: before ${JSON.stringify(before)}, after ${JSON.stringify(after)}`);
      }
      if (!zoomed && !(after.pagePointsInCanvas[0] > 100 && after.pagePointsInCanvas[1] > 100)) {
        throw new Error(`${what}: the page is ${after.pagePointsInCanvas} points in the canvas after: ${JSON.stringify(after)}`);
      }
    };
    const windowSize = (size) => page.setViewportSize(size);
    await shot("before");
    await round("the fitted view", windowSize, false);
    await shot("after");
    const zoom = await run(page, { command: "ui.zoom", params: { percent: 200 } });
    if (!zoom.ok) throw new Error(`ui.zoom: ${JSON.stringify(zoom).slice(0, 200)}`);
    await shot("zoomed-before");
    await round("a zoomed view", windowSize, true);
    await shot("zoomed-after");
    if (type === chromium) {
      // The browser's own zoom at 150 %: the window's size in CSS pixels divided by 1.5.
      const cdp = await context.newCDPSession(page);
      const zoomedWindow = (size) => cdp.send("Emulation.setDeviceMetricsOverride", {
        width: Math.round(size.width / 1.5), height: Math.round(size.height / 1.5), deviceScaleFactor: 1.5, mobile: false,
      });
      await zoomedWindow(big);
      await round("a zoomed view, the page zoomed 150 % by the browser", zoomedWindow, true);
      await shot("browser-zoom-after");
      await cdp.send("Emulation.clearDeviceMetricsOverride");
      inView("back from the browser's zoom", await view());
    }
    // The way back from the smallest window that has a canvas: Fit.
    await windowSize(small[1]);
    await view();
    const fit = await run(page, { command: "ui.zoom", params: { fit: "page" } });
    if (!fit.ok) throw new Error(`ui.zoom fit: ${JSON.stringify(fit).slice(0, 200)}`);
    await windowSize(big);
    const fitted = await view();
    inView("after Fit", fitted);
    if (!(fitted.pagePointsInCanvas[0] > 100 && fitted.pagePointsInCanvas[1] > 100)) {
      throw new Error(`after Fit the page is ${fitted.pagePointsInCanvas} points in the canvas: ${JSON.stringify(fitted)}`);
    }
    await shot("fit-after");
    noErrors("resize", errors);
  } finally {
    await context.close();
  }
}
