// IndexedDB as a cache, nothing more (docs/plans/030_web_client.md): what was
// last seen, so a reload draws at once and then pulls only what changed.
// Nothing plays from it.

let opened;

function open() {
  opened ??= new Promise((resolve, reject) => {
    const r = indexedDB.open("dhun", 1);
    r.onupgradeneeded = () => r.result.createObjectStore("kv");
    r.onsuccess = () => resolve(r.result);
    r.onerror = () => reject(r.error);
  });
  return opened;
}

async function run(mode, f) {
  const db = await open();
  return new Promise((resolve, reject) => {
    const t = db.transaction("kv", mode);
    const r = f(t.objectStore("kv"));
    t.oncomplete = () => resolve(r?.result);
    t.onerror = () => reject(t.error);
  });
}

/** The value kept under `key`, or undefined; a browser that refuses IndexedDB (private windows) keeps nothing. */
export const get = (key) => run("readonly", (s) => s.get(key)).catch(() => undefined);
export const set = (key, value) =>
  run("readwrite", (s) => s.put(value, key)).catch(() => {});
export const clear = () => run("readwrite", (s) => s.clear()).catch(() => {});
