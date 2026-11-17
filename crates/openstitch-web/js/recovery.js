// Autosave snapshots in the browser (#831): the editor's crash-recovery copies
// (openstitch_app::editor::recovery) live in this origin's IndexedDB, one record per tab:
// {id, info (JSON text), osd (the project file's bytes)}. Each tab holds a Web Lock named after
// its id for as long as it is open, so a record whose lock nobody holds was left by a tab that
// was closed or crashed, and is offered at the next visit.
//
// Every access is guarded: with storage refused (a private window, cleared or blocked site
// data, no IndexedDB) recoveryOpen gives null and the editor runs without recovery; a failed
// write or delete only warns.

const DB = "openstitch";
const STORE = "recovery";
const LOCK = "openstitch-recovery-";

let db = null;

function request(r) {
  return new Promise((ok, fail) => {
    r.onsuccess = () => ok(r.result);
    r.onerror = () => fail(r.error);
  });
}

function openDb() {
  const r = indexedDB.open(DB, 1);
  r.onupgradeneeded = () => {
    if (!r.result.objectStoreNames.contains(STORE)) {
      r.result.createObjectStore(STORE, { keyPath: "id" });
    }
  };
  return request(r);
}

// Ids are digits and one dash, as the desktop's `<process>-<n>` (recovery::check_id).
function newId() {
  const rand = Math.floor(Math.random() * 1e6).toString().padStart(6, "0");
  return `${Date.now()}${rand}-0`;
}

// Opens the store, takes this tab's lock and lists what closed or crashed tabs left:
// {own, locks, orphans: [{id, info, osd: Uint8Array}]}, or null when storage is refused.
export async function recoveryOpen() {
  try {
    if (typeof indexedDB === "undefined" || indexedDB === null) return null;
    db = await openDb();
    const own = newId();
    let held = new Set();
    let locks = false;
    try {
      const lm = navigator.locks;
      if (lm && typeof lm.request === "function" && typeof lm.query === "function") {
        // Held until the tab goes: the promise never settles.
        lm.request(LOCK + own, () => new Promise(() => {})).catch(() => {});
        const state = await lm.query();
        held = new Set((state.held ?? []).map((l) => l.name));
        locks = true;
      }
    } catch {
      // No Web Locks (an old browser): every other tab's record is offered.
    }
    const all = await request(db.transaction(STORE, "readonly").objectStore(STORE).getAll());
    const orphans = [];
    for (const r of all ?? []) {
      if (!r || typeof r.id !== "string" || r.id === own || held.has(LOCK + r.id)) continue;
      if (typeof r.info !== "string" || !(r.osd instanceof Uint8Array || r.osd instanceof ArrayBuffer)) continue;
      orphans.push({ id: r.id, info: r.info, osd: new Uint8Array(r.osd) });
    }
    return { own, locks, orphans };
  } catch (e) {
    console.warn("crash recovery is off: browser storage is refused", e);
    db = null;
    return null;
  }
}

function write(fn, what) {
  if (!db) return;
  try {
    const t = db.transaction(STORE, "readwrite");
    fn(t.objectStore(STORE));
    t.onerror = () => console.warn(`${what}: browser storage is full or refused`, t.error);
  } catch (e) {
    console.warn(`${what}: browser storage is full or refused`, e);
  }
}

// Writes this tab's snapshot (the bytes are copied).
export function recoveryWrite(id, osd, info) {
  write((s) => s.put({ id, info, osd: new Uint8Array(osd) }), "autosave");
}

// Removes one snapshot.
export function recoveryRemove(id) {
  write((s) => s.delete(id), "recovery");
}
