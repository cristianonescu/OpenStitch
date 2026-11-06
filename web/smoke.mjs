// Smoke test of the browser build (#556), run by CI job `web` in Node:
//   node web/smoke.mjs PKG_DIR NATIVE_DIR INPUT... -- FORMAT...
// PKG_DIR holds the `wasm-bindgen --target nodejs` output. For each INPUT it opens the file
// through `run` (ui.openFile), checks ui.state and the simulator, exports each FORMAT
// (ui.exportFile) and compares the bytes with NATIVE_DIR/<input name>.<format>, written by
// `cargo run -p openstitch-web --example web_export`. Then it saves and reopens the .osd.
import { readFileSync } from "node:fs";
import { basename, join, resolve } from "node:path";
import { createRequire } from "node:module";

const [pkg, nativeDir, ...rest] = process.argv.slice(2);
const split = rest.indexOf("--");
if (!pkg || !nativeDir || split < 1 || split === rest.length - 1) {
  console.error("usage: node web/smoke.mjs PKG_DIR NATIVE_DIR INPUT... -- FORMAT...");
  process.exit(2);
}
const inputs = rest.slice(0, split);
const formats = rest.slice(split + 1);
const web = createRequire(import.meta.url)(resolve(pkg, "openstitch_web.js"));

let failures = 0;
const fail = (msg) => {
  console.error(`FAIL ${msg}`);
  failures += 1;
};
const run = (command, params) => {
  const reply = JSON.parse(web.run(JSON.stringify({ command, params })));
  if (!reply.ok) throw new Error(`${command}: ${reply.error}`);
  return reply.result;
};

for (const input of inputs) {
  const name = basename(input);
  try {
    run("ui.openFile", { name, base64: readFileSync(input).toString("base64") });
    const state = run("ui.state");
    if (state.file !== name || state.stitches < 1) fail(`${name}: state ${JSON.stringify(state)}`);
    const sim = run("ui.simulate", { stitch: 1 });
    if (sim.stitch !== 1 || sim.total < 1) fail(`${name}: simulator ${JSON.stringify(sim)}`);
    for (const format of formats) {
      const files = run("ui.exportFile", { format }).files;
      const got = Buffer.from(files[0].base64, "base64");
      const want = readFileSync(join(nativeDir, `${name}.${format}`));
      if (files.length !== 1 || !got.equals(want)) {
        fail(`${name} as ${format}: ${got.length} bytes in the browser build, ${want.length} natively`);
      } else {
        console.log(`ok ${name} as ${format}: ${got.length} bytes, identical to the desktop build`);
      }
    }
    const saved = run("ui.saveFile");
    run("ui.openFile", { name: saved.name, base64: saved.base64 });
    if (run("ui.state").file !== saved.name) fail(`${name}: reopening ${saved.name}`);
    else console.log(`ok ${name}: saved and reopened as ${saved.name}`);
  } catch (e) {
    fail(`${name}: ${e.message}`);
  }
}
const bad = JSON.parse(web.run(JSON.stringify({ command: "ui.openFile", params: { name: "x.txt", base64: "" } })));
if (bad.ok) fail("a .txt file opened");
if (failures) process.exit(1);
console.log("web smoke test passed");
