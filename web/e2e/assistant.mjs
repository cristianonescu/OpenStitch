// Browser test of the web editor's Assistant (#766): Chromium, Firefox and WebKit through
// Playwright, against a fake model endpoint on another port (fake-model.mjs), so every request
// is a real cross-origin fetch from the page. No network and no real key.
//   node web/e2e/assistant.mjs [--optional BROWSER]... WEB_DIR [OUT_DIR]
// WEB_DIR holds index.html and pkg/ (wasm-bindgen --target web). In each browser:
//   1. The Claude API protocol, with a test key kept in memory only: "draw a box" runs the
//      model's execute shape.rectangle in the editor, the reply streams in, and the change is
//      one undo step. The request carried the key, the browser-access header and no cookie, and
//      the key is in neither the page's storage nor the preferences.
//   2. A file command asks first: the approval card comes up, Deny refuses it, Allow runs it
//      (the export arrives as a download).
//   3. "Remember on this device" puts the key in the page's storage; unticking removes it.
//   4. An OpenAI-compatible endpoint on this computer with no key: the same tool loop.
// A browser named with --optional that cannot start or has no WebGL is a CI warning.

import { createServer } from "node:http";
import { mkdirSync, readFileSync } from "node:fs";
import { extname, join, resolve, posix } from "node:path";
import { chromium, firefox, webkit } from "playwright";
import { startFakeModel } from "./fake-model.mjs";

const args = process.argv.slice(2);
const optional = new Set();
while (args[0] === "--optional") {
  args.shift();
  optional.add((args.shift() ?? "").toLowerCase());
}
const [webDir, outDir] = args;
if (!webDir) {
  console.error("usage: node web/e2e/assistant.mjs [--optional BROWSER]... WEB_DIR [OUT_DIR]");
  process.exit(2);
}
const root = resolve(webDir);
const types = { ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm" };
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
const model = await startFakeModel();
// A test value, never a real key.
const KEY = "test-key-for-the-page-test";

const run = (page, request) => page.evaluate((r) => window.openstitch.run(r), request);
const ok = async (page, request) => {
  const reply = await run(page, request);
  if (!reply.ok) throw new Error(`${request.command}: ${JSON.stringify(reply).slice(0, 300)}`);
  return reply.result;
};
const started = async (page) => {
  await page.waitForFunction(
    () => !document.getElementById("message") || /could not start/.test(document.getElementById("message").textContent),
    null,
    { timeout: 90_000 },
  );
  return page.evaluate(() => document.getElementById("message")?.textContent ?? "");
};
/// Waits until the turn has ended (or an approval card waits, with `card`); returns the state.
const settled = async (page, card = false) => {
  const handle = await page.waitForFunction(
    (card) => {
      const s = window.openstitch.run({ command: "ui.assistantState" }).result;
      return (card ? s.approval != null : !s.busy) && s;
    },
    card,
    { timeout: 30_000, polling: 100 },
  );
  return handle.jsonValue();
};
const objects = async (page) => (await ok(page, { command: "design.info" })).objects;
const storageHasKey = (page) =>
  page.evaluate((key) => Object.keys(localStorage).some((k) => (localStorage.getItem(k) ?? "").includes(key)), KEY);

let failures = 0;
for (const [label, type] of [["Chromium", chromium], ["Firefox", firefox], ["WebKit", webkit]]) {
  console.log(`${label}: launching`);
  let browser;
  try {
    browser = await type.launch({
      timeout: 60_000,
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
    const message = await started(page);
    if (message && /WebGL/i.test(message) && optional.has(label.toLowerCase())) {
      console.log(`::warning::${label} has no WebGL on this runner, not tested: ${message}`);
      continue;
    }
    if (message) throw new Error(message);
    model.requests.length = 0;

    // 1. The Claude API protocol, key in memory only.
    console.log(`${label}: Claude API protocol, a box`);
    await ok(page, { command: "ui.assistantSetup", params: { kind: "anthropic", address: model.address, model: "claude-test", key: KEY } });
    const before = await objects(page);
    // Before the first message the panel names the host that receives it.
    const host = new URL(model.address).host;
    const notice = (await ok(page, { command: "ui.assistantState" })).sendsTo;
    if (!notice.includes(host)) throw new Error(`the panel does not name ${host}: ${notice}`);
    // And says what it cannot do yet in the browser (#1224, #1223), in words, without issue numbers.
    const limits = (await ok(page, { command: "ui.assistantState" })).limits;
    if (!/see a picture/.test(limits) || !/outside AI clients/.test(limits) || /#\d/.test(limits)) throw new Error(`limits: ${limits}`);
    await ok(page, { command: "ui.assistantSend", params: { text: "draw a box" } });
    let s = await settled(page);
    const after = await objects(page);
    if (after !== before + 1) throw new Error(`objects ${before} → ${after}: ${JSON.stringify(s.items)}`);
    const said = s.items.map((i) => `${i.kind}:${i.text}`);
    for (const want of ["tool:Ran shape.rectangle", "notice:1 change, one undo step."]) {
      if (!said.includes(want)) throw new Error(`no "${want}" in ${JSON.stringify(said)}`);
    }
    if (!said.some((t) => t.startsWith("assistant:Done:"))) throw new Error(`no streamed answer in ${JSON.stringify(said)}`);
    const sent = model.requests.filter((r) => r.method === "POST");
    if (sent.length !== 2) throw new Error(`${sent.length} model requests, want 2`);
    for (const r of sent) {
      if (r.headers["x-api-key"] !== KEY) throw new Error("the request did not carry the key");
      if (r.headers["anthropic-dangerous-direct-browser-access"] !== "true") throw new Error("no browser-access header");
      if (r.headers.cookie) throw new Error("the request carried cookies");
    }
    if (await storageHasKey(page)) throw new Error("the key is in the page's storage without Remember");
    if (JSON.stringify(await ok(page, { command: "ui.settings" })).includes(KEY)) throw new Error("the key is in the preferences");

    // 2. A file command waits for the card: Deny, then Allow.
    for (const allow of [false, true]) {
      console.log(`${label}: an export waits for the approval card; ${allow ? "Allow" : "Deny"}`);
      await ok(page, { command: "ui.assistantSend", params: { text: "export the design" } });
      s = await settled(page, true);
      if (!s.approval.some((w) => /read or write files|assistant\.dst/.test(w))) throw new Error(`card: ${JSON.stringify(s.approval)}`);
      const download = allow ? page.waitForEvent("download", { timeout: 15_000 }).catch(() => null) : null;
      await ok(page, { command: "ui.assistantAnswer", params: { allow } });
      s = await settled(page);
      const last = s.items.filter((i) => i.kind === "assistant").at(-1)?.text ?? "";
      if (allow) {
        const d = await download;
        if (!d) throw new Error(`no download after Allow: ${JSON.stringify(s.items.slice(-4))}`);
        if (d.suggestedFilename() !== "assistant.dst") throw new Error(`downloaded ${d.suggestedFilename()}`);
        if (!/approv/.test(last)) throw new Error(`the result did not say it was approved: ${last}`);
      } else if (!/declined/.test(last)) {
        throw new Error(`the result did not say it was declined: ${last}`);
      }
    }

    // One undo step takes the box away (the export changed nothing).
    await ok(page, { command: "history.undo" });
    if ((await objects(page)) !== before) throw new Error("one undo did not take the message's change back");

    // 2b. A model cannot point the panel elsewhere or drive it (#766 review). Its server does not
    // know ui.assistantSetup; the panel commands it does know (Send, Answer) reach the gate, which
    // refuses them, inside run_batch too. Nothing runs and the endpoint stays.
    for (const [text, refused] of [["redirect yourself", /ui\.assistantSetup/], ["send yourself", /only the user/]]) {
      console.log(`${label}: the model tries "${text}"`);
      const users = s.items.filter((i) => i.kind === "user").length;
      await ok(page, { command: "ui.assistantSend", params: { text } });
      s = await settled(page);
      const last = s.items.filter((i) => i.kind === "assistant").at(-1)?.text ?? "";
      const tool = s.items.filter((i) => i.kind === "tool").at(-1);
      if (!refused.test(last) || /"ok":true/.test(last)) throw new Error(`not refused: ${last}`);
      if (text === "redirect yourself" && !tool?.error) throw new Error(`the call did not fail: ${JSON.stringify(tool)}`);
      if (s.items.filter((i) => i.kind === "user").length !== users + 1) throw new Error("the model sent itself a message");
      if (!s.sendsTo.includes(host)) throw new Error(`the endpoint moved: ${s.sendsTo}`);
      if (/elsewhere/.test(JSON.stringify(s))) throw new Error("the planted address reached the panel");
    }
    // The page's replies never hold the key.
    for (const command of ["ui.assistantState"]) {
      if (JSON.stringify(await ok(page, { command })).includes(KEY)) throw new Error(`${command} returned the key`);
    }
    if (JSON.stringify(await ok(page, { command: "ui.assistantSetup", params: { kind: "anthropic", address: model.address, model: "claude-test", key: KEY } })).includes(KEY)) {
      throw new Error("ui.assistantSetup returned the key");
    }

    // 3. Remember on this device, then not.
    console.log(`${label}: Remember on this device`);
    await ok(page, { command: "ui.assistantSetup", params: { kind: "anthropic", address: model.address, model: "claude-test", key: KEY, remember: true } });
    if (!(await storageHasKey(page))) throw new Error("Remember did not keep the key in the page's storage");
    await ok(page, { command: "ui.assistantSetup", params: { kind: "anthropic", address: model.address, model: "claude-test", key: KEY, remember: false } });
    if (await storageHasKey(page)) throw new Error("unticking Remember left the key in the page's storage");
    // Keys remembered for two addresses; Forget all removes both.
    await ok(page, { command: "ui.assistantSetup", params: { kind: "anthropic", address: model.address, model: "claude-test", key: KEY, remember: true } });
    await ok(page, { command: "ui.assistantSetup", params: { kind: "openai", address: `${model.address}/v1`, model: "local-test", key: `${KEY}-2`, remember: true } });
    const forgot = await ok(page, { command: "ui.assistantForgetKeys" });
    if (forgot.forgotten !== 2 || (await storageHasKey(page))) throw new Error(`Forget all: ${JSON.stringify(forgot)}`);

    // 4. OpenAI-compatible on this computer, no key. The key typed for the Claude address above
    // belongs to that address: it is not sent here.
    console.log(`${label}: OpenAI-compatible endpoint, no key`);
    await ok(page, { command: "ui.assistantSetup", params: { kind: "anthropic", address: model.address, model: "claude-test", key: KEY } });
    model.requests.length = 0;
    await ok(page, { command: "ui.assistantSetup", params: { kind: "openai", address: `${model.address}/v1`, model: "local-test" } });
    const b2 = await objects(page);
    await ok(page, { command: "ui.assistantSend", params: { text: "draw a box" } });
    s = await settled(page);
    if ((await objects(page)) !== b2 + 1) throw new Error(`OpenAI-compatible: no box: ${JSON.stringify(s.items.slice(-4))}`);
    if (!model.requests.some((r) => r.path === "/v1/chat/completions")) throw new Error("no chat/completions request");
    // Every tool row keeps its own label, though the fake reuses one call id.
    const rows = s.items.filter((i) => i.kind === "tool").map((i) => i.text).join(", ");
    if (rows !== "Ran shape.rectangle, Ran design.export, Ran design.export, Ran ui.assistantSetup, Ran 2 commands, Ran shape.rectangle") throw new Error(`tool rows: ${rows}`);
    if (model.requests.some((r) => r.headers.authorization)) throw new Error("a key went to a keyless local endpoint");

    if (outDir) {
      mkdirSync(outDir, { recursive: true });
      await page.screenshot({ path: join(outDir, `assistant-${label.toLowerCase()}.png`) });
    }
    if (errors.length) throw new Error(`page errors: ${errors.join("; ")}`);
    console.log(`ok ${label}: the Assistant ran a tool loop on both protocols from the page, asked before a file command, and kept the key only where told`);
  } catch (e) {
    console.error(`FAIL ${label}: ${e.message}`);
    failures += 1;
  } finally {
    await browser.close().catch(() => {});
  }
}
model.close();
server.closeAllConnections?.();
server.close();
if (failures) process.exit(1);
console.log("web Assistant test passed");
process.exit(0);
