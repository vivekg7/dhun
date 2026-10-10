// The app's state and every change to it, the Mac's AppModel, Store and Sync
// in one (plan 030). Each change is applied here at once and recorded as an
// op in the outbox, which is sent within a second; the server's answer then
// overwrites what it covers. The outbox is kept in localStorage, so an edit
// made just before a reload is not lost (AGENTS.md).

import { Catalog, song as toSong } from "./catalog.js";
import * as db from "./db.js";
import { nowMs, parseTime, formatTime, uuid, debounce, distinct } from "./util.js";

export class ApiError extends Error {
  constructor(status, message, code = "") {
    super(message);
    this.status = status;
    this.code = code;
  }
}

const prefsKey = "dhun.prefs";
const outboxKey = "dhun.outbox";

/** Kept when another account signs in on this browser: they are the browser's, not the account's. */
const browserPrefs = ["theme", "palette", "speed", "miniPlayer"];

/** Raised when songs gain a field the cache lacks; 1 is the art key (plan 019). */
const libraryFormat = 1;

// A blip is over in seconds; the 15 minutes this once grew to on the phone
// left it offline long after the network was back (plan 019).
const retryMin = 5_000;
const retryMax = 120_000;

/** A browser's name for itself, for hand-off ("Continue from Chrome on macOS"). */
function deviceName() {
  const ua = navigator.userAgent;
  const browser = /Edg\//.test(ua)
    ? "Edge"
    : /Firefox\//.test(ua)
      ? "Firefox"
      : /Chrome\//.test(ua)
        ? "Chrome"
        : /Safari\//.test(ua)
          ? "Safari"
          : "Browser";
  const os = /iPhone|iPad/.test(ua)
    ? "iOS"
    : /Android/.test(ua)
      ? "Android"
      : /Mac OS X/.test(ua)
        ? "macOS"
        : /Windows/.test(ua)
          ? "Windows"
          : /Linux/.test(ua)
            ? "Linux"
            : "";
  return os ? `${browser} on ${os}` : browser;
}

export class App extends EventTarget {
  constructor() {
    super();
    this.prefs = load(prefsKey, {});
    this.outbox = load(outboxKey, []);
    this.catalog = new Catalog([]);
    this.rawSongs = [];
    this.playlists = [];
    this.queues = [];
    this.marks = { fav: new Map(), later: new Map() };
    this.resumeMap = new Map();
    this.settings = {};
    this.playStats = new Map();
    this.nowPlaying = null;
    this.thumbs = new Map();
    /** A playlist made here and the server id it became, for a page still showing the old one. */
    this.replaced = new Map();
    this.reachable = true;
    this.syncing = false;
    this.syncError = null;
    /** Signed in on the server; a refused token keeps everything, outbox included (plan 023). */
    this.signedIn = !!this.prefs.token;
    this.revoked = !this.signedIn && !!this.prefs.user;
    this.saveData = debounce(() => this.persist(), 800);
    this.saveThumbs = debounce(() => db.set("thumbs", this.thumbs), 2000);
    this.running = false;
    this.again = false;
    this.retryDelay = retryMin;
    this.pending = 0;
  }

  /** Tells the pages what changed: catalog, playlists, queues, marks, resumes, settings, stats, sync, nowPlaying, thumbs, account. */
  emit(topic) {
    this.dispatchEvent(new CustomEvent("change", { detail: topic }));
  }

  setPref(name, value) {
    if (value === undefined || value === "" || value === null) delete this.prefs[name];
    else this.prefs[name] = value;
    localStorage.setItem(prefsKey, JSON.stringify(this.prefs));
  }

  get user() {
    return this.prefs.user ?? "";
  }

  get admin() {
    return !!this.prefs.admin;
  }

  /** A synced setting with the app's default for missing or null (plan 009). */
  setting(name, def) {
    const v = this.settings[name];
    return v === undefined || v === null ? def : v;
  }

  // MARK: Start, the cache

  /** Draws from what was kept, then syncs. */
  async start() {
    if (!this.prefs.user) return;
    const [songs, data, thumbs] = await Promise.all([
      db.get("songs"),
      db.get("data"),
      db.get("thumbs"),
    ]);
    if (data?.user === this.prefs.user) {
      this.playlists = data.playlists ?? [];
      this.queues = data.queues ?? [];
      this.marks = data.marks ?? this.marks;
      this.resumeMap = data.resumes ?? this.resumeMap;
      this.settings = data.settings ?? {};
      this.playStats = data.playStats ?? this.playStats;
      // The cursors as kept with the data, never ahead of it; without the songs, the library comes again whole.
      this.setPref("libraryVersion", songs ? (data.libraryVersion ?? 0) : 0);
      this.setPref("syncVersion", data.syncVersion ?? 0);
      if (songs) this.setSongs(songs);
    } else {
      // Another account's, or nothing: start the cursors over.
      this.setPref("libraryVersion", 0);
      this.setPref("syncVersion", 0);
    }
    if (thumbs instanceof Map) this.thumbs = thumbs;
    for (const t of [
      "playlists",
      "queues",
      "marks",
      "resumes",
      "settings",
      "stats",
      "thumbs",
    ])
      this.emit(t);
  }

  /**
   * Writes the kept copy. The sync cursors go in the same record as the data
   * they cover: a cursor saved ahead of its data, with the tab then closed,
   * would skip what the server sent for good.
   */
  persist() {
    db.set("data", {
      user: this.prefs.user,
      libraryVersion: this.prefs.libraryVersion ?? 0,
      syncVersion: this.prefs.syncVersion ?? 0,
      playlists: this.playlists,
      queues: this.queues,
      marks: this.marks,
      resumes: this.resumeMap,
      settings: this.settings,
      playStats: this.playStats,
    });
  }

  setSongs(raw) {
    this.rawSongs = raw;
    this.catalog = new Catalog(raw);
    this.emit("catalog");
  }

  changed(topic) {
    this.emit(topic);
    this.saveData();
  }

  // MARK: The API (api/openapi.yaml)

  async call(method, path, body) {
    let res;
    try {
      res = await fetch(path, {
        method,
        cache: "no-store",
        headers: {
          ...(this.prefs.token ? { Authorization: `Bearer ${this.prefs.token}` } : {}),
          ...(body ? { "Content-Type": "application/json" } : {}),
        },
        body: body ? JSON.stringify(body) : undefined,
      });
    } catch {
      this.setReachable(false);
      throw new ApiError(0, "The server cannot be reached");
    }
    this.setReachable(true);
    const version = res.headers.get("Dhun-Version");
    if (version && version !== this.prefs.serverVersion)
      this.setPref("serverVersion", version);
    if (!res.ok) {
      const e = await res.json().catch(() => ({}));
      throw new ApiError(res.status, e.message || res.statusText, e.code);
    }
    return res.status === 204 ? {} : res.json();
  }

  setReachable(ok) {
    if (this.reachable === ok) return;
    this.reachable = ok;
    this.emit("sync");
    if (ok && this.outbox.length) this.soon(0);
  }

  // MARK: Account

  /** Signs in; another user's data is wiped first, the same user's kept with its unsent edits. */
  async signIn(user, password) {
    const login = await this.call("POST", "/api/v1/login", {
      username: user,
      password,
      device: deviceName(),
    });
    if (this.prefs.user && this.prefs.user.toLowerCase() !== user.toLowerCase())
      await this.forget();
    this.setPref("user", login.user?.name ?? user);
    this.setPref("admin", !!login.user?.admin);
    this.setPref("deviceId", login.deviceId ?? 0);
    this.setPref("token", login.token);
    this.signedIn = true;
    this.revoked = false;
    this.syncError = null;
    this.emit("account");
    await this.syncNow();
  }

  /** Signs out: the token goes, and with it this account's data in this browser. */
  async signOut() {
    try {
      await this.call("POST", "/api/v1/logout");
    } catch {
      // Signed out here whatever the server says; it can remove the device later.
    }
    await this.forget();
    this.setPref("token");
    this.signedIn = false;
    this.emit("account");
  }

  async forget() {
    this.dispatchEvent(new Event("reset"));
    await db.clear();
    const kept = Object.fromEntries(
      browserPrefs.filter((k) => k in this.prefs).map((k) => [k, this.prefs[k]]),
    );
    this.prefs = kept;
    localStorage.setItem(prefsKey, JSON.stringify(kept));
    this.outbox = [];
    localStorage.removeItem(outboxKey);
    this.playlists = [];
    this.queues = [];
    this.marks = { fav: new Map(), later: new Map() };
    this.resumeMap = new Map();
    this.settings = {};
    this.playStats = new Map();
    this.nowPlaying = null;
    this.thumbs = new Map();
    this.setSongs([]);
  }

  /** The token was refused: sign in again, but keep everything (plan 023). */
  tokenRevoked() {
    this.setPref("token");
    this.signedIn = false;
    this.revoked = true;
    this.emit("account");
  }

  // MARK: Store: a local write and an op

  /**
   * Records an op. `key` replaces earlier ops with the same key, where only
   * the latest matters. `tag` marks an op as touching a queue or playlist, so
   * a sync does not overwrite it with an older server copy while it is unsent.
   */
  record(type, fields, { key, tag, at = nowMs() } = {}) {
    const id = uuid();
    const op = { ...fields, id, type, at: formatTime(at) };
    if (key) this.outbox = this.outbox.filter((o) => o.key !== key);
    this.outbox.push({ id, key: key ?? (tag ? `${id}:${tag}` : id), op });
    localStorage.setItem(outboxKey, JSON.stringify(this.outbox));
    this.soon();
  }

  mark(kind, song, on) {
    this.marks[kind].set(song, { at: nowMs(), deleted: !on });
    const type =
      kind === "fav"
        ? on
          ? "favorite.set"
          : "favorite.unset"
        : on
          ? "listen_later.add"
          : "listen_later.remove";
    this.record(type, { song }, { key: `${kind}:${song}` });
    this.changed("marks");
  }

  isMarked(kind, song) {
    const m = this.marks[kind].get(song);
    return !!m && !m.deleted;
  }

  /** The songs of Favorites or Listen Later, newest first (plan 010). */
  marked(kind) {
    return [...this.marks[kind]]
      .filter(([, m]) => !m.deleted)
      .sort((a, b) => b[1].at - a[1].at)
      .map(([song]) => song);
  }

  setSetting(name, value) {
    if (value === null) delete this.settings[name];
    else this.settings[name] = value;
    this.record("setting.set", { name, value }, { key: `setting:${name}` });
    this.changed("settings");
  }

  /** Where a long file was left, or null once it was heard to the end (plan 009). */
  setResume(song, positionMs) {
    this.resumeMap.set(song, {
      positionMs: positionMs ?? 0,
      at: nowMs(),
      deleted: positionMs == null,
    });
    if (positionMs != null)
      this.record("resume.set", { song, positionMs }, { key: `resume:${song}` });
    else this.record("resume.unset", { song }, { key: `resume:${song}` });
    this.changed("resumes");
  }

  resume(song) {
    const r = this.resumeMap.get(song);
    return r && !r.deleted && r.positionMs > 0 ? r.positionMs : 0;
  }

  /** Continue listening: the long files left part-way, the latest first. */
  resumes() {
    return [...this.resumeMap]
      .filter(([, r]) => !r.deleted && r.positionMs > 0)
      .sort((a, b) => b[1].at - a[1].at)
      .map(([song]) => song);
  }

  // Queues. `putQueue` is the player's own bookkeeping (usedAt), needing no op.

  queue(id) {
    return this.queues.find((q) => q.id === id);
  }

  putQueue(q) {
    const i = this.queues.findIndex((x) => x.id === q.id);
    if (i < 0) this.queues.push({ ...q });
    else this.queues[i] = { ...q };
    this.changed("queues");
  }

  createQueue(q) {
    this.putQueue(q);
    this.record(
      "queue.create",
      {
        queue: q.id,
        name: q.name,
        songs: q.songs,
        song: q.currentSong,
        positionMs: q.positionMs,
      },
      { tag: q.id },
    );
  }

  replaceQueue(q) {
    this.putQueue(q);
    this.record(
      "queue.replace",
      { queue: q.id, songs: q.songs },
      { key: `replace:${q.id}` },
    );
  }

  renameQueue(q) {
    this.putQueue(q);
    this.record(
      "queue.rename",
      { queue: q.id, name: q.name },
      { key: `rename:${q.id}` },
    );
  }

  deleteQueue(id) {
    this.queues = this.queues.filter((q) => q.id !== id);
    this.record("queue.delete", { queue: id }, { tag: id });
    this.changed("queues");
  }

  /** The queue's current song and position; only the latest is worth sending. */
  setCurrent(q) {
    this.putQueue(q);
    this.record(
      "queue.set_current",
      { queue: q.id, song: q.currentSong, positionMs: q.positionMs },
      { key: `current:${q.id}` },
    );
  }

  setMode(q) {
    this.putQueue(q);
    this.record(
      "queue.set_mode",
      { queue: q.id, shuffle: q.shuffle, repeat: q.repeat },
      { key: `mode:${q.id}` },
    );
  }

  insertIntoQueue(q, songs, after) {
    this.putQueue(q);
    this.record(
      "queue.insert",
      { queue: q.id, songs, ...(after == null ? {} : { after }) },
      { tag: q.id },
    );
  }

  removeFromQueue(q, songs) {
    this.putQueue(q);
    this.record("queue.remove", { queue: q.id, songs }, { tag: q.id });
  }

  moveInQueue(q, song, after) {
    this.putQueue(q);
    this.record("queue.move", { queue: q.id, song, after }, { tag: q.id });
  }

  // Playlists (plan 013). One made here has a negative id until the server's
  // comes back with the same ref; ops name it by "ref:<ref>" meanwhile.

  playlist(id) {
    return this.playlists.find((p) => p.id === id);
  }

  target(p) {
    return p.id > 0 ? String(p.id) : `ref:${p.ref}`;
  }

  putPlaylist(p) {
    const i = this.playlists.findIndex((x) => x.id === p.id);
    if (i < 0) this.playlists.push(p);
    else this.playlists[i] = p;
    this.playlists.sort(
      (a, b) => Number(b.shared) - Number(a.shared) || a.name.localeCompare(b.name),
    );
    this.changed("playlists");
  }

  createPlaylist(name, songs) {
    const p = {
      id: -Math.floor(Math.random() * 2 ** 52) - 1,
      name,
      path: "",
      shared: false,
      songs: distinct(songs),
      ref: uuid(),
    };
    this.putPlaylist(p);
    this.record(
      "playlist.create",
      { ref: p.ref, name, songs: p.songs },
      { tag: `pl${p.id}` },
    );
    return p;
  }

  /** Adds `songs` at the end, skipping those already in it; returns how many were skipped. */
  addToPlaylist(p, songs) {
    const have = new Set(p.songs);
    const unique = distinct(songs);
    const adding = unique.filter((s) => !have.has(s));
    if (adding.length) {
      this.putPlaylist({ ...p, songs: [...p.songs, ...adding] });
      this.record(
        "playlist.insert",
        { playlist: this.target(p), songs: adding },
        { tag: `pl${p.id}` },
      );
    }
    return unique.length - adding.length;
  }

  /** Removes the entry at `index` of the playlist's own list (unmatched entries included). */
  removeFromPlaylist(p, index) {
    const song = p.songs[index];
    const occurrence = p.songs.slice(0, index).filter((s) => s === song).length;
    const songs = p.songs.slice();
    songs.splice(index, 1);
    this.putPlaylist({ ...p, songs });
    this.record(
      "playlist.remove",
      { playlist: this.target(p), song, occurrence },
      { tag: `pl${p.id}` },
    );
  }

  /** Moves the entry at `from` to `to`, both indexes into the playlist's own list. */
  moveInPlaylist(p, from, to) {
    if (from === to) return;
    const list = p.songs.slice();
    const song = list[from];
    const occurrence = list.slice(0, from).filter((s) => s === song).length;
    list.splice(to, 0, ...list.splice(from, 1));
    this.putPlaylist({ ...p, songs: list });
    const after = to === 0 ? 0 : list[to - 1];
    if (to > 0 && after === 0) {
      // An unmatched entry cannot be named as "after" (0 means the start): send the order.
      this.record(
        "playlist.replace",
        { playlist: this.target(p), songs: list },
        { tag: `pl${p.id}` },
      );
      return;
    }
    const f = { playlist: this.target(p), song, occurrence, after };
    if (to > 0)
      f.afterOccurrence = list.slice(0, to - 1).filter((s) => s === after).length;
    this.record("playlist.move", f, { tag: `pl${p.id}` });
  }

  /** The whole order at once (a sort or reverse). */
  replacePlaylist(p, songs) {
    this.putPlaylist({ ...p, songs });
    this.record(
      "playlist.replace",
      { playlist: this.target(p), songs },
      { tag: `pl${p.id}` },
    );
  }

  renamePlaylist(p, name) {
    this.putPlaylist({ ...p, name });
    this.record(
      "playlist.rename",
      { playlist: this.target(p), name },
      { tag: `pl${p.id}` },
    );
  }

  /** The server keeps a copy of the file. */
  deletePlaylist(p) {
    this.playlists = this.playlists.filter((x) => x.id !== p.id);
    this.record("playlist.delete", { playlist: this.target(p) }, { tag: `pl${p.id}` });
    this.changed("playlists");
  }

  // Playback

  /** Feeds /now-playing, for "Continue from …" on another device. */
  playbackState(queue, song, positionMs, playing) {
    this.record(
      "playback.state",
      { queue, song, positionMs, playing },
      { key: "playback" },
    );
  }

  /** One listen (plan 008). */
  play(l) {
    const f = {
      song: l.song,
      ms: l.ms,
      endedAt: formatTime(l.endedAt),
      fromMs: l.fromMs,
      toMs: l.toMs,
      end: l.end,
      shuffle: l.shuffle,
      utcOffset: l.utcOffset,
    };
    if (l.source) f.source = l.source;
    if (l.queue) f.queue = l.queue;
    this.record("play", f, { at: l.startedAt });
  }

  // MARK: Sync (plan 006)

  /** Debounced: a burst of clicks becomes one request. A failed sync retries with a growing delay. */
  soon(ms = 1000) {
    clearTimeout(this.pending);
    this.pending = setTimeout(() => this.runRetrying(), ms);
  }

  async runRetrying() {
    if (await this.syncNow()) {
      this.retryDelay = retryMin;
    } else {
      this.retryDelay = Math.min(this.retryDelay * 2, retryMax);
      this.soon(this.retryDelay);
    }
  }

  /** Returns false when the server could not be reached, so `soon` tries again later. */
  async syncNow() {
    if (!this.signedIn) return true;
    // One at a time; a request while one runs makes it run once more.
    if (this.running) {
      this.again = true;
      return true;
    }
    this.running = true;
    this.syncing = true;
    this.emit("sync");
    let ok = true;
    try {
      do {
        this.again = false;
        ok = await this.round();
      } while (this.again && ok);
    } finally {
      this.running = false;
      this.syncing = false;
      this.emit("sync");
    }
    return ok;
  }

  async round() {
    try {
      // A field added to songs comes only with the songs the server sends
      // again, and it would send none that had not changed.
      if ((this.prefs.libraryFormat ?? 0) < libraryFormat)
        this.setPref("libraryVersion", 0);
      await this.pullLibrary();
      this.setPref("libraryFormat", libraryFormat);
      this.pushedPlaylists = false;
      // More than 500 ops take several rounds.
      while ((await this.pushAndPull()) > 0 && this.outbox.length > 0);
      // The server rewrote those playlists: fetch them as written.
      if (this.pushedPlaylists) await this.pullLibrary();
      await this.pullPlays();
      this.syncError = null;
      this.fetchThumbs();
      return true;
    } catch (e) {
      // A revoked token (password reset, device removed): sign in again, but
      // keep everything, the unsent edits included (plan 023).
      if (e.status === 401) this.tokenRevoked();
      this.syncError = e.message;
      return !(e.status === 0 || e.status >= 500);
    }
  }

  editedPlaylist(id) {
    return this.outbox.some((o) => o.key.endsWith(`:pl${id}`));
  }

  /**
   * Songs and playlists changed since the cursor. A playlist with an op still
   * in the outbox is left as edited here, and the cursor stays put so the
   * next pull brings it again once the op has gone (plan 013).
   */
  async pullLibrary() {
    const since = this.prefs.libraryVersion ?? 0;
    const lib = await this.call("GET", `/api/v1/library?since=${since}`);
    const songs = (lib.songs ?? []).map(toSong);
    if (songs.length) {
      const byId = new Map(since === 0 ? [] : this.rawSongs.map((s) => [s.id, s]));
      for (const s of songs) byId.set(s.id, s);
      const all = [...byId.values()];
      db.set("songs", all);
      this.setSongs(all);
    }
    let skipped = false;
    for (const p of lib.playlists ?? []) {
      if (p.deleted) {
        if (this.editedPlaylist(p.id)) skipped = true;
        else this.playlists = this.playlists.filter((x) => x.id !== p.id);
        continue;
      }
      const ref = p.ref ?? "";
      const local = this.playlists.filter((x) => x.id < 0);
      // One made here, back with its real id. A server older than `ref` is matched by name, once its create has gone.
      const mine =
        local.find((x) => ref && x.ref === ref) ??
        local.find(
          (x) => !ref && !p.shared && x.name === p.name && !this.editedPlaylist(x.id),
        );
      if (mine) {
        if (this.editedPlaylist(mine.id)) {
          skipped = true;
          continue;
        }
        this.playlists = this.playlists.filter((x) => x.id !== mine.id);
        this.replaced.set(mine.id, p.id);
      } else if (this.editedPlaylist(p.id)) {
        skipped = true;
        continue;
      }
      this.putPlaylist({
        id: p.id,
        name: p.name ?? "",
        path: p.path ?? "",
        shared: !!p.shared,
        songs: p.songs ?? [],
        ref,
      });
    }
    if (!skipped) this.setPref("libraryVersion", lib.version ?? since);
    this.persist();
    this.changed("playlists");
  }

  /** One round trip; returns how many ops the server answered. */
  async pushAndPull() {
    const ops = this.outbox.slice(0, 500);
    if (ops.some((o) => o.op.type.startsWith("playlist."))) this.pushedPlaylists = true;
    const state = await this.call("POST", "/api/v1/sync", {
      since: this.prefs.syncVersion ?? 0,
      ops: ops.map((o) => o.op),
    });
    // applied, duplicate and rejected all leave the outbox: a rejected op can never succeed.
    const results = state.results ?? [];
    const done = new Set(results.map((r) => r.id));
    // A playlist the server refused to create (a name it reserves) would otherwise stay here forever.
    for (const r of results) {
      if (r.status !== "rejected") continue;
      const o = ops.find((x) => x.id === r.id);
      if (o?.op.type === "playlist.create") {
        const id = Number(o.key.slice(o.key.lastIndexOf(":pl") + 3));
        this.playlists = this.playlists.filter((p) => p.id !== id);
        this.changed("playlists");
      }
    }
    if (done.size) {
      this.outbox = this.outbox.filter((o) => !done.has(o.id));
      localStorage.setItem(outboxKey, JSON.stringify(this.outbox));
    }
    this.apply(state);
    this.setPref("syncVersion", state.version ?? this.prefs.syncVersion);
    this.persist();
    return done.size;
  }

  /** Sends what is waiting while the page closes; the server ignores an op it already has. */
  flush() {
    if (!this.signedIn || !this.outbox.length) return;
    fetch("/api/v1/sync", {
      method: "POST",
      keepalive: true,
      headers: {
        Authorization: `Bearer ${this.prefs.token}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        since: this.prefs.syncVersion ?? 0,
        ops: this.outbox.slice(0, 200).map((o) => o.op),
      }),
    }).catch(() => {});
  }

  /**
   * Applies what the server says changed. Anything with an op still in the
   * outbox (made while the request was in flight) is left alone: that op goes
   * with the next sync, and the server's answer will include it.
   */
  apply(s) {
    const pending = new Set(this.outbox.map((o) => o.key));
    const pendingFor = (id) => this.outbox.some((o) => o.key.endsWith(`:${id}`));
    for (const q of s.queues ?? []) {
      if (q.deleted) {
        this.queues = this.queues.filter((x) => x.id !== q.id);
        continue;
      }
      if (pendingFor(q.id)) continue;
      const row = {
        id: q.id,
        name: q.name ?? "",
        songs: q.songs ?? [],
        currentSong: q.currentSong ?? 0,
        positionMs: q.positionMs ?? 0,
        shuffle: !!q.shuffle,
        repeat: q.repeat ?? "off",
        usedAt: parseTime(q.usedAt),
      };
      const i = this.queues.findIndex((x) => x.id === q.id);
      if (i < 0) this.queues.push(row);
      else this.queues[i] = row;
    }
    for (const [kind, items] of [
      ["fav", s.favorites ?? []],
      ["later", s.listenLater ?? []],
    ]) {
      for (const i of items) {
        const at = parseTime(i.at);
        if (pending.has(`${kind}:${i.song}`)) continue;
        const local = this.marks[kind].get(i.song);
        if (local && local.at > at) continue;
        this.marks[kind].set(i.song, { at, deleted: !!i.deleted });
      }
    }
    for (const r of s.resume ?? []) {
      const at = parseTime(r.at);
      if (pending.has(`resume:${r.song}`)) continue;
      const local = this.resumeMap.get(r.song);
      if (local && local.at > at) continue;
      this.resumeMap.set(r.song, {
        positionMs: r.positionMs ?? 0,
        at,
        deleted: !!r.deleted,
      });
    }
    for (const [name, value] of Object.entries(s.settings ?? {})) {
      if (pending.has(`setting:${name}`)) continue;
      if (value === null) delete this.settings[name];
      else this.settings[name] = value;
    }
    // Sent only when it changed since the cursor: no news is not "nothing playing".
    if (s.nowPlaying) {
      this.nowPlaying = s.nowPlaying;
      this.emit("nowPlaying");
    }
    // Marked, so the player can tell another device's change from its own.
    this.applying = true;
    try {
      for (const t of ["queues", "marks", "resumes", "settings"]) this.emit(t);
    } finally {
      this.applying = false;
    }
    this.saveData();
  }

  async pullPlays() {
    const r = await this.call("GET", "/api/v1/plays");
    this.playStats = new Map(
      (r.plays ?? []).map((p) => [
        p.song,
        { song: p.song, count: p.count ?? 0, lastPlayedAt: parseTime(p.lastPlayedAt) },
      ]),
    );
    this.changed("stats");
  }

  /** What the user's devices played last, asked directly: after a reload the sync cursor is already past it. */
  async checkHandoff() {
    if (!this.signedIn) return;
    try {
      if (!this.prefs.deviceId || this.prefs.admin === undefined) {
        const me = await this.call("GET", "/api/v1/me");
        this.setPref("deviceId", me.deviceId ?? 0);
        this.setPref("admin", !!me.user?.admin);
      }
      const r = await this.call("GET", "/api/v1/now-playing");
      if (r.nowPlaying) {
        this.nowPlaying = r.nowPlaying;
        this.emit("nowPlaying");
      }
    } catch {
      // Unreachable: nothing to hand off from this time.
    }
  }

  // MARK: Covers (plan 019)

  /** A list row's cover: the 128 px thumbnail kept for every cover, as a data: URL; "" when the song has none. */
  thumb(s) {
    if (!s?.art) return "";
    return this.thumbs.get(s.art) ?? null;
  }

  /** A larger cover, by URL: the browser caches it, and the cookie lets <img> through. */
  cover(s, px) {
    return s?.art
      ? `/api/v1/art/${s.id}?size=${px}&k=${encodeURIComponent(s.art)}`
      : "";
  }

  /** Fetches the thumbnails not kept yet, 50 at a time; a key that failed is asked again next time. */
  async fetchThumbs() {
    if (this.fetchingThumbs) return;
    this.fetchingThumbs = true;
    try {
      const want = distinct(
        this.catalog.songs.map((s) => s.art).filter((k) => k && !this.thumbs.has(k)),
      );
      for (let i = 0; i < want.length; i += 50) {
        const r = await this.call("POST", "/api/v1/thumbs", {
          keys: want.slice(i, i + 50),
        });
        for (const [k, b64] of Object.entries(r.thumbs ?? {}))
          this.thumbs.set(k, b64 ? `data:image/jpeg;base64,${b64}` : "");
        this.emit("thumbs");
        this.saveThumbs();
      }
    } catch {
      // Unreachable or refused: the rest come with the next sync.
    } finally {
      this.fetchingThumbs = false;
    }
  }

  // MARK: Lyrics and the admin's calls

  /** A song's lyrics text, or null when it has none; kept for the session. */
  async lyrics(song) {
    this.lyricsCache ??= new Map();
    if (this.lyricsCache.has(song)) return this.lyricsCache.get(song);
    try {
      const r = await this.call("GET", `/api/v1/lyrics/${song}`);
      this.lyricsCache.set(song, r.text ?? "");
      return r.text ?? "";
    } catch (e) {
      if (e.status === 404) {
        this.lyricsCache.set(song, null);
        return null;
      }
      throw e;
    }
  }

  /** Songs whose lyrics hold the words, with the line (plan 029). */
  async searchLyrics(q) {
    const r = await this.call(
      "GET",
      `/api/v1/search/lyrics?q=${encodeURIComponent(q)}`,
    );
    return r.hits ?? [];
  }

  members() {
    return this.call("GET", "/api/v1/admin/users").then((r) => r.users ?? []);
  }

  addMember(name, password) {
    return this.call("POST", "/api/v1/admin/users", { name, password });
  }

  resetPassword(user, password) {
    return this.call("PUT", `/api/v1/admin/users/${user}/password`, { password });
  }

  devices() {
    return this.call("GET", "/api/v1/devices").then((r) => r.devices ?? []);
  }

  removeDevice(id) {
    return this.call("DELETE", `/api/v1/devices/${id}`);
  }

  // MARK: This browser's searches (plan 029)

  get recentSearches() {
    return this.prefs.recentSearches ?? [];
  }

  /** Kept when something it found is opened or played, not on every keystroke. */
  keepSearch(query) {
    const q = query.trim();
    if (!q) return;
    const rest = this.recentSearches.filter((s) => s.toLowerCase() !== q.toLowerCase());
    this.setPref("recentSearches", [q, ...rest].slice(0, 10));
  }

  forgetSearches() {
    this.setPref("recentSearches");
  }
}

function load(key, def) {
  try {
    return JSON.parse(localStorage.getItem(key)) ?? def;
  } catch {
    return def;
  }
}
