// The player and the queues around it, the Mac's Playback (plans 007, 024,
// 030). The browser plays; this holds the active queue's play order and
// loads only that queue, so switching saves the outgoing queue's song and
// position and loads the incoming one where it was left. Every listen is
// logged (plan 008) and long files keep a resume point (plan 009).
//
// Two <audio> elements take turns: one plays, the other opens the next song
// while it does, and starts the moment the first ends. That is close to
// gapless, not sample-exact (docs/plans/030_web_client.md).

import {
  maxQueues,
  orderSetting,
  orderOf,
  playOrder,
  nextIn,
  tempoOf,
  songSetting,
} from "./queue.js";
import { nowMs, parseTime, uuid, distinct } from "./util.js";

/** A hand-off never starts closer than this to the song's end. */
const endMarginMs = 5000;
/** How long the volume takes to fall to nothing before a sleep timer by the clock pauses. */
const fadeMs = 10_000;
/** The next song is opened this far from the current one's end. */
const preloadSeconds = 30;

export class Playback extends EventTarget {
  constructor(app) {
    super();
    this.app = app;
    this.els = [new Audio(), new Audio()];
    for (const el of this.els) {
      el.preload = "auto";
      el.preservesPitch = true;
      el.addEventListener("ended", () => el === this.el && this.songEnded());
      el.addEventListener("error", () => this.mediaError(el));
      el.addEventListener("waiting", () => el === this.el && this.setWaiting(true));
      el.addEventListener("playing", () => el === this.el && this.setWaiting(false));
      el.addEventListener("loadedmetadata", () => el === this.el && this.emit());
      // Paused from outside: headphones out on a phone, the OS's controls.
      el.addEventListener(
        "pause",
        () =>
          el === this.el && this.isPlaying && !el.ended && !this.ours && this.pause(),
      );
    }
    this.cur = 0;
    this.activeId = app.prefs.activeQueue ?? "";
    this.current = null;
    this.isPlaying = false;
    this.waiting = false;
    /** The active queue's songs as loaded, in queue order; `order` is the play order of the same ids. */
    this.items = [];
    this.order = [];
    this.position = 0;
    this.restored = false;
    this.ended = false;
    this.failures = 0;
    this.open = null;
    this.ticks = 0;
    /** A long file with a resume point, waiting for the user's answer ("ask", plan 009). */
    this.offerResume = null;
    /** A note for the user (a queue removed to make room), shown until dismissed. */
    this.notice = null;
    this.sleep = { mode: null };
    this.pauseAfter = null;
    this.stateAt = app.prefs.stateAt ?? 0;
    this.closeInterrupted();
    this.applyTempo();
    setInterval(() => this.tick(), 250);
    app.addEventListener("reset", () => this.reset());
    app.addEventListener("change", (e) => {
      if (e.detail === "catalog") this.catalogChanged();
      if (e.detail === "queues") this.queuesChanged();
      if (e.detail === "settings") this.applyTempo();
    });
    this.mediaSession();
  }

  get el() {
    return this.els[this.cur];
  }

  get spare() {
    return this.els[1 - this.cur];
  }

  /** "state" for anything but the position, which changes four times a second. */
  emit(kind = "state") {
    this.dispatchEvent(new Event(kind));
  }

  // MARK: Derived state

  get active() {
    return this.app.queue(this.activeId);
  }

  /** The queues in the user's order, numbered from 1 as in Musicolet (plan 020). */
  get ordered() {
    return orderOf(this.app.queues, this.app.settings[orderSetting]);
  }

  get shuffle() {
    return this.active?.shuffle ?? false;
  }

  get repeat() {
    return this.active?.repeat ?? "off";
  }

  get duration() {
    if (this.current?.durationMs > 0) return this.current.durationMs / 1000;
    return Number.isFinite(this.el.duration) ? this.el.duration : 0;
  }

  /** The playing song's own speed, synced; null when it follows this browser's everyday one. */
  get songTempo() {
    return this.current
      ? tempoOf(this.app.settings[songSetting(this.current.id)])
      : null;
  }

  get speed() {
    return this.songTempo?.speed ?? this.app.prefs.speed ?? 1;
  }

  get currentIndex() {
    return this.current ? this.items.indexOf(this.current.id) : -1;
  }

  get hasNext() {
    return this.nextIndex(this.currentIndex, this.repeat === "queue") >= 0;
  }

  /**
   * "Continue from MacBook": another device's playback, offered while this
   * one is not playing, when it is newer than anything played here and its
   * queue and song are here (plan 017).
   */
  get handoff() {
    const np = this.app.nowPlaying;
    const me = this.app.prefs.deviceId ?? 0;
    if (!np || this.isPlaying || !me || np.deviceId === me) return null;
    if (parseTime(np.at) <= this.stateAt || np.at === this.app.prefs.handoffDismissed)
      return null;
    if (!this.app.queue(np.queue) || !this.app.catalog.byId.has(np.song)) return null;
    return np;
  }

  // MARK: Transport

  toggle() {
    this.isPlaying ? this.pause() : this.play();
  }

  play() {
    const s = this.current;
    if (!s) return;
    if (this.ended) {
      // The queue ran out: play its song again, as Media3 does on Play.
      return this.start(s, 0, false);
    }
    if (this.needsRestart) {
      // The server could not be reached: open the song again where it was.
      return this.start(s, this.position, false);
    }
    this.setPlaying(true);
    this.resumeElement();
  }

  pause() {
    if (!this.isPlaying) return;
    this.quietly(() => this.el.pause());
    this.setPlaying(false);
  }

  next() {
    const n = this.nextIndex(this.currentIndex, this.repeat === "queue");
    if (n >= 0) this.jump(n, "skipped");
  }

  /** Back to the start of the song, or to the one before within its first three seconds. */
  previous() {
    const i = this.currentIndex;
    if (i < 0) return;
    if (this.position > 3) return this.seek(0);
    const p = this.previousIndex(i);
    if (p < 0) return this.seek(0);
    this.jump(p, "previous");
  }

  seek(seconds) {
    const s = this.current;
    if (!s) return;
    this.position = Math.max(0, Math.min(seconds, this.duration || seconds));
    // Paused, nothing else saves where it now is: a reload or the other devices would get the old place.
    if (!this.isPlaying && this.active)
      this.savePlace(this.active, s, Math.round(this.position * 1000));
    if (this.ended) {
      this.start(s, this.position, !this.isPlaying);
    } else {
      this.el.currentTime = this.position;
    }
    this.emit("position");
    this.updateSession();
  }

  /** Plays the song at `index` of the active queue's own order. */
  playAt(index) {
    if (index >= 0 && index < this.items.length) this.jump(index, "switched", true);
  }

  // MARK: Queues

  /**
   * Plays `songs` from `start` in a new queue named `name` (AGENTS.md:
   * playing from a list never overwrites the current queue). A queue already
   * called `name` is reused and refilled rather than duplicated, so playing
   * songs in one album does not leave "Album (2)", "Album (3)".
   */
  playList(name, source, songs, start = 0) {
    if (!songs.length) return;
    this.saveActive();
    const first = songs[Math.min(Math.max(start, 0), songs.length - 1)];
    const ids = distinct(songs.map((s) => s.id));
    const now = nowMs();
    let q = this.app.queues.find((x) => x.name.toLowerCase() === name.toLowerCase());
    if (q) {
      q = { ...q, songs: ids, currentSong: first.id, positionMs: 0, usedAt: now };
      this.app.replaceQueue(q);
      this.app.setCurrent(q);
    } else {
      q = this.create(name, ids, first.id, now);
    }
    this.app.setPref(`source.${q.id}`, source);
    this.load(q, true);
  }

  /** A new queue, last in the order. At most 20: as in Musicolet, the first goes to make room, and a note says so. */
  create(name, ids, current, now) {
    const order = this.ordered;
    const over = Math.max(0, order.length - (maxQueues - 1));
    // The first queues go, but never the one playing.
    const gone = order.filter((q) => q.id !== this.activeId).slice(0, over);
    for (const q of gone) this.app.deleteQueue(q.id);
    if (gone.length)
      this.say(`At most ${maxQueues} queues: “${gone[0].name}” was removed`);
    const q = {
      id: uuid(),
      name,
      songs: ids,
      currentSong: current,
      positionMs: 0,
      shuffle: false,
      repeat: "off",
      usedAt: now,
    };
    this.app.createQueue(q);
    const left = new Set(gone.map((x) => x.id));
    this.setOrder([...order.filter((x) => !left.has(x.id)).map((x) => x.id), q.id]);
    return q;
  }

  /** A queue of `songs` that does not start playing. The name is made unique the way the server does it. */
  newQueue(name, songs) {
    const taken = new Set(this.app.queues.map((q) => q.name.toLowerCase()));
    let unique = name;
    for (let n = 2; taken.has(unique.toLowerCase()); n++) unique = `${name} (${n})`;
    const adding = distinct(
      songs.filter((s) => this.app.catalog.byId.has(s.id)).map((s) => s.id),
    );
    this.create(unique, adding, adding[0] ?? 0, nowMs());
  }

  setOrder(ids) {
    this.app.setSetting(orderSetting, ids);
  }

  /** Drag in the queue list: queue `from` goes to `to`, counting from 0. */
  moveQueue(from, to) {
    const ids = this.ordered.map((q) => q.id);
    if (from === to || !ids[from] || !ids[to]) return;
    ids.splice(to, 0, ...ids.splice(from, 1));
    this.setOrder(ids);
  }

  /** Every queue but `keep` (Musicolet's "Remove all other queues"). */
  deleteOthers(keep) {
    for (const q of this.app.queues.slice())
      if (q.id !== keep) this.app.deleteQueue(q.id);
    this.setOrder([keep]);
    if (keep !== this.activeId) this.switchTo(keep);
  }

  switchTo(id) {
    const q = this.app.queue(id);
    if (!q) return;
    if (id === this.activeId) return this.play();
    this.saveActive();
    this.load({ ...q, usedAt: nowMs() }, true);
  }

  /** After the current song; songs already queued are moved, never duplicated. */
  playNext(songs) {
    this.insert(songs, true);
  }

  addToQueue(songs) {
    this.insert(songs, false);
  }

  insert(songs, next) {
    const q = this.active;
    if (!q) return this.playList("Queue", "", songs, 0);
    const playing = this.current?.id;
    const adding = distinct(
      songs
        .map((s) => s.id)
        .filter((id) => id !== playing && this.app.catalog.byId.has(id)),
    );
    if (!adding.length) return;
    const set = new Set(adding);
    this.items = this.items.filter((id) => !set.has(id));
    const at = next ? this.currentIndex + 1 : this.items.length;
    this.items.splice(at, 0, ...adding);
    this.rebuildOrder(true);
    this.app.insertIntoQueue(
      { ...q, songs: this.items.slice() },
      adding,
      at === 0 ? 0 : this.items[at - 1],
    );
    this.say(
      next
        ? `${plural(adding.length)} to play next`
        : `${plural(adding.length)} added to the queue`,
    );
    this.updateSleep();
  }

  /** Adds `songs` to the end of queue `id`; songs already in it move there. */
  addTo(id, songs) {
    if (id === this.activeId) return this.addToQueue(songs);
    const q = this.app.queue(id);
    if (!q) return;
    const adding = distinct(
      songs.map((s) => s.id).filter((s) => this.app.catalog.byId.has(s)),
    );
    if (!adding.length) return;
    const set = new Set(adding);
    const row = { ...q, songs: [...q.songs.filter((s) => !set.has(s)), ...adding] };
    if (!row.currentSong) row.currentSong = row.songs[0];
    this.app.insertIntoQueue(row, adding, null);
    this.say(`${plural(adding.length)} added to “${q.name}”`);
  }

  removeFrom(id, songs) {
    const q = this.app.queue(id);
    if (!songs.size || !q) return;
    if (id === this.activeId) {
      const wasCurrent = this.current && songs.has(this.current.id);
      const i = this.currentIndex;
      const following =
        i >= 0 ? this.items.slice(i + 1).find((s) => !songs.has(s)) : undefined;
      this.items = this.items.filter((s) => !songs.has(s));
      this.rebuildOrder(true);
      this.app.removeFromQueue({ ...q, songs: this.items.slice() }, [...songs]);
      if (wasCurrent) {
        // The player goes on with the song after it, as Media3 does.
        this.close("skipped");
        const n = following ?? this.items[0];
        const s = n && this.app.catalog.byId.get(n);
        if (s) {
          this.start(s, 0, !this.isPlaying);
          this.afterSongChange(s, 0);
        } else this.stopAll();
      }
      this.updateSleep();
      return;
    }
    const old = q.songs;
    const row = { ...q, songs: old.filter((s) => !songs.has(s)) };
    this.app.removeFromQueue(row, [...songs]);
    if (songs.has(q.currentSong)) {
      const from = old.indexOf(q.currentSong) + 1;
      row.currentSong = old.slice(from).find((s) => !songs.has(s)) ?? row.songs[0] ?? 0;
      row.positionMs = 0;
      this.app.setCurrent(row);
    }
  }

  /**
   * Moves song `from` of queue `id` to `to`, counting from 0 among the songs
   * shown: a queue that is not playing may hold songs no longer in the
   * library, which keep their places.
   */
  moveIn(id, from, to) {
    const q = this.app.queue(id);
    if (from === to || !q) return;
    const list = (id === this.activeId ? this.items : q.songs).slice();
    const shown = list.filter((s) => this.app.catalog.byId.has(s));
    const song = shown[from];
    const target = shown[to];
    if (song == null || target == null) return;
    const before = list.indexOf(song) < list.indexOf(target);
    list.splice(list.indexOf(song), 1);
    list.splice(list.indexOf(target) + (before ? 1 : 0), 0, song);
    const at = list.indexOf(song);
    if (id === this.activeId) {
      this.items = list;
      this.rebuildOrder(true);
      this.updateSleep();
    }
    this.app.moveInQueue({ ...q, songs: list }, song, at === 0 ? 0 : list[at - 1]);
  }

  /** Queue `id` in a new order of the same songs (a sort, Randomize, Reverse). The playing song plays on. */
  reorder(id, newOrder) {
    const q = this.app.queue(id);
    if (!q) return;
    // Songs no longer in the library are not shown or sorted; they stay in the queue, at its end.
    const gone = q.songs.filter((x) => !this.app.catalog.byId.has(x));
    if (id === this.activeId) {
      const loaded = new Set(this.items);
      this.items = newOrder.filter((s) => loaded.has(s));
      this.rebuildOrder(true);
      this.updateSleep();
      this.app.replaceQueue({ ...q, songs: [...this.items, ...gone] });
    } else {
      this.app.replaceQueue({ ...q, songs: [...newOrder, ...gone] });
    }
  }

  rename(id, name) {
    const q = this.app.queue(id);
    if (q) this.app.renameQueue({ ...q, name });
  }

  delete(id) {
    this.setOrder(this.ordered.map((q) => q.id).filter((x) => x !== id));
    if (id === this.activeId) {
      this.close("stopped");
      // Gone before stopAll(), so the active id is cleared when no queue is left.
      this.app.deleteQueue(id);
      this.stopAll();
      // The one used most recently takes its place, paused.
      const next = this.app.queues.slice().sort((a, b) => b.usedAt - a.usedAt)[0];
      if (next) this.load(next, false);
    } else {
      this.app.deleteQueue(id);
    }
  }

  toggleShuffle() {
    const q = this.active;
    if (!q) return;
    this.app.setMode({ ...q, shuffle: !q.shuffle });
    this.rebuildOrder(false);
    this.emit();
  }

  /** off → whole queue → this song → off. */
  cycleRepeat() {
    const q = this.active;
    if (!q) return;
    this.app.setMode({
      ...q,
      repeat: { off: "queue", queue: "song" }[q.repeat] ?? "off",
    });
    this.updateSleep();
    this.preloadNext();
    this.emit();
  }

  answerResume(accept) {
    const o = this.offerResume;
    if (!o) return;
    this.offerResume = null;
    if (accept && this.current?.id === o.song.id) this.seek(o.ms / 1000);
    this.emit();
  }

  say(text) {
    this.notice = text;
    this.emit();
  }

  // MARK: Speed (plan 016)

  /** For the playing song only (synced, as its own setting), or as this browser's everyday one. */
  setSpeed(speed, onlyThisSong) {
    if (onlyThisSong && this.current) {
      const kept = this.songTempo ?? { semitones: 0 };
      this.app.setSetting(songSetting(this.current.id), {
        speed,
        semitones: kept.semitones,
      });
    } else {
      this.app.setPref("speed", speed === 1 ? undefined : speed);
    }
    this.applyTempo();
  }

  /** The playing song follows the everyday speed again. */
  clearSongSpeed() {
    if (this.current) this.app.setSetting(songSetting(this.current.id), null);
    this.applyTempo();
  }

  applyTempo() {
    // Time heard so far counts at the old speed.
    if (this.open) this.count(this.open);
    this.heardSpeed = this.speed;
    for (const el of this.els) {
      el.defaultPlaybackRate = this.heardSpeed;
      el.playbackRate = this.heardSpeed;
    }
    this.updateSession();
    this.emit();
  }

  // MARK: Hand-off (plan 017)

  /** Where a hand-off continues: the other device's place, moved on by the time passed if it was still playing. */
  placeOf(np) {
    let pos = np.positionMs ?? 0;
    if (np.playing) pos += Math.max(0, nowMs() - parseTime(np.at));
    const length = this.app.catalog.byId.get(np.song)?.durationMs ?? 0;
    if (length > 0) pos = Math.min(pos, length - endMarginMs);
    return Math.max(0, pos);
  }

  /** Takes over from another device: its queue (queues are synced), its song, and its place. */
  continueFrom(np) {
    const q = this.app.queue(np.queue);
    const s = this.app.catalog.byId.get(np.song);
    if (!q || !s) return;
    const pos = this.placeOf(np);
    if (q.id !== this.activeId) this.saveActive();
    this.load({ ...q, currentSong: s.id, positionMs: pos, usedAt: nowMs() }, true, pos);
  }

  /** ✕ on the offer: not offered again, though a newer playback on that device will be. */
  dismissHandoff(np) {
    this.app.setPref("handoffDismissed", np.at);
    this.emit();
  }

  /** Tells the server what this browser plays, for the others' hand-off. */
  report(q, s, posMs, playing) {
    const now = nowMs();
    this.stateAt = now;
    this.app.setPref("stateAt", now);
    this.app.playbackState(q.id, s.id, posMs, playing);
  }

  /** On sign-out: the open listen belongs to the account being left, and is dropped. */
  reset() {
    this.cancelSleep();
    this.open = null;
    this.app.setPref("openListen");
    this.stateAt = 0;
    this.stopAll();
    this.offerResume = null;
    this.restored = false;
  }

  // MARK: Loading

  /** The catalogue arrived: load the queue that was playing when the page last closed, paused, once. */
  catalogChanged() {
    if (this.restored || !this.app.catalog.songs.length) return;
    this.restored = true;
    const q = this.active;
    if (!this.current && q) this.load(q, false);
  }

  /**
   * The queues changed, here or on another device. The playing queue may be
   * gone (deleted elsewhere), or hold other songs (added on the phone): the
   * player follows, so a later edit here does not undo theirs.
   */
  queuesChanged() {
    // Edits made here keep the player in step themselves.
    if (!this.activeId || !this.restored || !this.app.applying) return;
    const q = this.active;
    if (!q) {
      if (!this.current) return this.setActive("");
      this.close("stopped");
      this.stopAll();
      this.say("The queue that was playing was removed on another device");
      return;
    }
    const fresh = this.app.catalog.songsOf(q.songs).map((s) => s.id);
    const same =
      fresh.length === this.items.length &&
      fresh.every((id, i) => id === this.items[i]);
    if (same || (this.current && !fresh.includes(this.current.id))) return;
    this.items = fresh;
    this.rebuildOrder(true);
    this.updateSleep();
  }

  load(q, play, position) {
    this.close("switched");
    const songs = this.app.catalog.songsOf(q.songs);
    this.setActive(q.id);
    if (!songs.length) return this.stopAll();
    const index = Math.max(
      0,
      songs.findIndex((s) => s.id === q.currentSong),
    );
    const song = songs[index];
    let start = position;
    if (start != null) this.offerResume = null;
    start ??= this.startPosition(song);
    start ??= song.id === q.currentSong ? q.positionMs : 0;
    // Saved at the very end (a sleep timer stopped it there): start over rather than finish it again at once.
    if (song.durationMs > 0 && start >= song.durationMs - 1000) start = 0;
    this.items = songs.map((s) => s.id);
    this.order = [];
    this.app.putQueue(q);
    this.rebuildOrder(false, q.shuffle, song.id);
    this.start(song, start / 1000, !play);
  }

  /** Opens `s` at `seconds`; the listen opens with it. */
  start(s, seconds, paused) {
    this.current = s;
    this.position = seconds;
    this.ended = false;
    this.needsRestart = false;
    this.applyTempo();
    this.updateSleep();
    this.openListen(s, Math.round(seconds * 1000));
    const el = this.el;
    if (el.dataset.song !== String(s.id)) {
      el.dataset.song = String(s.id);
      // A media fragment opens it at the place, before the first byte arrives.
      el.src = `/api/v1/stream/${s.id}${seconds > 0 ? `#t=${seconds}` : ""}`;
    } else {
      el.currentTime = seconds;
    }
    this.clearSpare();
    this.setPlaying(!paused);
    if (!paused) this.resumeElement();
    else this.quietly(() => el.pause());
    this.updateSession();
    this.emit();
  }

  /** play(), and a browser that refuses (no click yet on this page) shows paused rather than stuck. */
  resumeElement() {
    const el = this.el;
    el.play().catch((e) => {
      if (e.name === "NotAllowedError") this.setPlaying(false);
      else if (e.name === "NotSupportedError") this.mediaError(el);
    });
  }

  /** Pauses an element without that pause being taken for one from outside. */
  quietly(f) {
    this.ours = true;
    f();
    queueMicrotask(() => (this.ours = false));
  }

  jump(index, end, play) {
    const s = this.app.catalog.byId.get(this.items[index]);
    if (!s) return;
    const finished = this.current;
    const at = Math.round(this.position * 1000);
    this.close(end, at);
    if (finished) this.saveResume(finished, at);
    this.onNextSongSleep();
    let start = this.startPosition(s) ?? 0;
    if (s.id === finished?.id) start = 0;
    this.start(s, start / 1000, !(play ?? this.isPlaying));
    this.afterSongChange(s, start);
  }

  afterSongChange(s, startMs) {
    const q = this.active;
    if (!q) return;
    this.app.setCurrent({ ...q, currentSong: s.id, positionMs: startMs });
    // The next song, for a hand-off from another device.
    if (this.isPlaying) this.report(q, s, startMs, true);
  }

  stopAll() {
    for (const el of this.els) {
      this.quietly(() => el.pause());
      el.removeAttribute("src");
      delete el.dataset.song;
      el.load();
    }
    this.items = [];
    this.order = [];
    this.current = null;
    this.position = 0;
    this.setPlaying(false);
    if (this.activeId && !this.app.queue(this.activeId)) this.setActive("");
    this.updateSession();
    this.emit();
  }

  setActive(id) {
    this.activeId = id;
    this.app.setPref("activeQueue", id);
  }

  /** Records where the outgoing queue was, before another is loaded. */
  saveActive() {
    const q = this.active;
    const s = this.current;
    if (!q || !s) return;
    const pos = Math.round(this.position * 1000);
    this.app.setCurrent({ ...q, currentSong: s.id, positionMs: pos });
    this.saveResume(s, pos);
  }

  // MARK: Play order

  rebuildOrder(keep, shuffle = this.shuffle, current = this.current?.id) {
    this.order = playOrder(this.items, this.order, current, shuffle, keep);
    this.preloadNext();
  }

  /** The index in `items` of the song after `items[i]` in play order, or -1. */
  nextIndex(i, wrap) {
    if (i < 0) return -1;
    const id = nextIn(this.order, this.items[i], wrap ? "queue" : "off");
    return id == null ? -1 : this.items.indexOf(id);
  }

  previousIndex(i) {
    const at = this.order.indexOf(this.items[i]);
    return at > 0 ? this.items.indexOf(this.order[at - 1]) : -1;
  }

  /** The song the player goes on to by itself, or null: the end of the queue, or the sleep timer's song. */
  upcoming() {
    const s = this.current?.id;
    if (s == null || this.pauseAfter === s) return null;
    return nextIn(this.order, s, this.repeat);
  }

  /** Opens the next song in the spare element, near the end of this one, so it starts at once. */
  preloadNext() {
    const next = this.upcoming();
    const spare = this.spare;
    if (next == null || next === this.current?.id) return this.clearSpare();
    if (this.duration - this.position > preloadSeconds) return;
    if (spare.dataset.song === String(next)) return;
    spare.dataset.song = String(next);
    spare.src = `/api/v1/stream/${next}`;
    spare.load();
  }

  clearSpare() {
    const spare = this.spare;
    if (!spare.dataset.song) return;
    delete spare.dataset.song;
    spare.removeAttribute("src");
    spare.load();
  }

  // MARK: Following the player

  tick() {
    if (!this.current || !this.isPlaying) return;
    this.position = this.el.currentTime;
    this.emit("position");
    this.preloadNext();
    this.ticks++;
    // Every 10 s of playing: save the open listen; every 30 s, the positions.
    if (this.ticks % 40 === 0) this.saveListen();
    if (this.ticks % 120 === 0 && this.active) {
      const q = this.active;
      const s = this.current;
      const pos = Math.round(this.position * 1000);
      this.app.setCurrent({ ...q, currentSong: s.id, positionMs: pos });
      this.saveResume(s, pos);
      this.report(q, s, pos, true);
    }
    if (this.ticks % 4 === 0) this.updatePosition();
  }

  /** The song ended: on to the next by itself, or the end of the queue. */
  songEnded() {
    const finished = this.current;
    const next = this.upcoming();
    const s = next == null ? null : this.app.catalog.byId.get(next);
    if (!s) return this.reachedEnd();
    this.close(
      "finished",
      finished.durationMs > 0 ? finished.durationMs : Math.round(this.position * 1000),
    );
    // Heard to the end: a long file's resume point is done with.
    if (this.isLong(finished)) this.app.setResume(finished.id, null);
    this.onNextSongSleep();
    const start = this.startPosition(s);
    if (s.id === finished.id) {
      this.el.currentTime = 0;
      this.resumeElement();
      this.current = s;
      this.position = 0;
    } else if (
      this.spare.dataset.song === String(s.id) &&
      start == null &&
      !this.spare.error &&
      this.spare.readyState > 0
    ) {
      // The next song is open already: it starts now, which is what keeps the gap small.
      this.cur = 1 - this.cur;
      this.el.playbackRate = this.speed;
      this.resumeElement();
      this.clearSpare();
      this.current = s;
      this.position = 0;
    } else {
      this.start(s, (start ?? 0) / 1000, false);
    }
    this.failures = 0;
    this.applyTempo();
    this.openListen(s, start ?? 0);
    this.afterSongChange(s, start ?? 0);
    this.updateSleep();
    this.updateSession();
    this.emit();
  }

  /** Everything has played: the end of the queue, or a sleep timer's song. */
  reachedEnd() {
    const s = this.current;
    this.close(
      "finished",
      s.durationMs > 0 ? s.durationMs : Math.round(this.position * 1000),
    );
    if (this.isLong(s)) this.app.setResume(s.id, null);
    const bySleep = this.pauseAfter != null && this.sleep.mode;
    this.setPlaying(false);
    const n = this.nextIndex(this.items.indexOf(s.id), this.repeat === "queue");
    const next = n >= 0 && this.app.catalog.byId.get(this.items[n]);
    this.pausedAtEndSleep();
    if (bySleep && next) {
      // Paused where the song ended; Play goes on with the next one.
      this.start(next, 0, true);
      this.afterSongChange(next, 0);
      return;
    }
    this.ended = true;
    this.emit();
  }

  /**
   * An element failed. The spare is only forgotten: the song opens again in
   * its turn. For the playing one, the server is asked: a song it serves is
   * one this browser cannot play, and is passed over; otherwise the server is
   * down or this browser signed out, and skipping would walk the whole queue.
   */
  async mediaError(el) {
    if (el !== this.el) {
      delete el.dataset.song;
      return;
    }
    const song = el.dataset.song;
    if (!song) return;
    let status = 0;
    try {
      const r = await fetch(`/api/v1/stream/${song}`, {
        headers: { Range: "bytes=0-0" },
      });
      status = r.status;
      r.body?.cancel();
    } catch {
      // Unreachable.
    }
    if (el.dataset.song !== song) return;
    if (status === 200 || status === 206) return this.songFailed();
    if (status === 401) this.app.tokenRevoked();
    // Held where it was: Play opens the song again.
    delete el.dataset.song;
    this.needsRestart = true;
    this.setPlaying(false);
    this.say(
      status === 401
        ? "Signed out: sign in again to play"
        : "The server cannot be reached",
    );
  }

  /** A song that cannot be played is passed over, as the apps do; a queue of them is not tried forever. */
  songFailed() {
    this.failures++;
    const n = this.nextIndex(this.currentIndex, this.repeat === "queue");
    if (this.failures >= Math.max(1, this.items.length) || n < 0) {
      this.setPlaying(false);
      this.say(`“${this.current?.title}” cannot be played in this browser`);
      return;
    }
    this.jump(n, "skipped");
  }

  setWaiting(w) {
    if (this.waiting === w) return;
    this.waiting = w;
    this.emit();
  }

  setPlaying(playing) {
    const changed = playing !== this.isPlaying;
    this.isPlaying = playing;
    // Not on a skip past a song that failed, which plays on: the count must reach the queue's length.
    if (playing && changed) this.failures = 0;
    const o = this.open;
    if (o) {
      if (playing) this.started(o);
      else if (o.since > 0) {
        this.count(o);
        o.since = 0;
      }
    }
    if (!changed) return;
    this.emit();
    const q = this.active;
    const s = this.current;
    if (!q || !s) return;
    const pos = Math.round(this.position * 1000);
    this.report(q, s, pos, playing);
    if (!playing) this.savePlace(q, s, pos);
    this.updateSession();
  }

  /** Where queue `q` is, kept and synced: on pause, and on a seek while paused. */
  savePlace(q, s, pos) {
    this.app.setCurrent({ ...q, currentSong: s.id, positionMs: pos });
    this.saveResume(s, pos);
  }

  // MARK: Long files (plan 009)

  isLong(s) {
    return s.durationMs >= this.app.setting("longFiles.minMinutes", 15) * 60_000;
  }

  /** Where to start `s`: its resume point under "auto"; under "ask", offered instead. */
  startPosition(s) {
    this.offerResume = null;
    const ms = this.isLong(s) ? this.app.resume(s.id) : 0;
    if (!ms) return null;
    switch (this.app.setting("longFiles.resume", "auto")) {
      case "auto":
        return ms;
      case "ask":
        this.offerResume = { song: s, ms };
        return null;
      default:
        return null;
    }
  }

  saveResume(s, pos) {
    if (this.isLong(s) && pos > 0) this.app.setResume(s.id, pos);
  }

  // MARK: Listens (plan 008)

  openListen(s, from) {
    const q = this.active;
    this.open = {
      song: s.id,
      startedAt: 0,
      fromMs: from,
      queue: q?.id ?? "",
      source: q ? (this.app.prefs[`source.${q.id}`] ?? "") : "",
      shuffle: this.shuffle,
      heardMs: 0,
      lastPos: from,
      lastAt: 0,
      since: 0,
    };
    if (this.isPlaying) this.started(this.open);
  }

  started(o) {
    o.since = performance.now();
    if (!o.startedAt) o.startedAt = nowMs();
  }

  /**
   * Adds the time played since the last count, as song time: two minutes at
   * 1.5× heard three minutes of the song, which is what the play count's
   * "half the song heard" compares (plan 008).
   */
  count(o) {
    if (!(o.since > 0)) return;
    const now = performance.now();
    o.heardMs += Math.round((now - o.since) * (this.heardSpeed ?? 1));
    o.since = now;
  }

  close(end, toMs) {
    const o = this.open;
    if (!o) return;
    this.open = null;
    this.app.setPref("openListen");
    this.count(o);
    // A song that never actually played was not a listen.
    if (o.heardMs > 0)
      this.app.play(
        this.listen(o, end, toMs ?? Math.round(this.position * 1000), nowMs()),
      );
  }

  listen(o, end, toMs, endedAt) {
    return {
      song: o.song,
      startedAt: o.startedAt,
      endedAt,
      ms: o.heardMs,
      fromMs: o.fromMs,
      toMs,
      end,
      source: o.source,
      queue: o.queue,
      shuffle: o.shuffle,
      utcOffset: -new Date(o.startedAt).getTimezoneOffset(),
    };
  }

  saveListen() {
    const o = this.open;
    if (!o) return;
    this.count(o);
    o.lastPos = Math.round(this.position * 1000);
    o.lastAt = nowMs();
    const { since, ...kept } = o;
    this.app.setPref("openListen", kept);
  }

  /** A listen still open when the page last closed (or crashed) is closed as interrupted. */
  closeInterrupted() {
    const o = this.app.prefs.openListen;
    if (!o) return;
    this.app.setPref("openListen");
    if (o.heardMs > 0)
      this.app.play(this.listen(o, "interrupted", o.lastPos, o.lastAt));
  }

  /** The page is closing or reloading: the open listen and the positions are saved, as on pause. */
  leaving() {
    if (this.isPlaying) {
      const q = this.active;
      const s = this.current;
      const pos = Math.round(this.position * 1000);
      if (q && s) {
        this.report(q, s, pos, false);
        this.savePlace(q, s, pos);
      }
    }
    this.saveListen();
  }

  // MARK: The sleep timer (plan 014)

  /**
   * After some minutes, at the end of this song, after a number of songs, at
   * the end of the queue, or after a song picked in the queue (plan 020). A
   * timer by the clock fades out over its last seconds and pauses on time;
   * the others pause where a song ends, which needs no fade.
   * Modes: {kind: "at", end}, {kind: "song"}, {kind: "songs", left}, {kind: "queue"}, {kind: "after", song, title}.
   */
  sleepMinutes(m) {
    const end = nowMs() + m * 60_000;
    this.setSleep({ kind: "at", end });
    const step = () => {
      if (this.sleep.mode?.kind !== "at" || this.sleep.mode.end !== end) return;
      const left = end - nowMs();
      if (left <= 0) {
        this.pause();
        this.cancelSleep();
        return;
      }
      for (const el of this.els) el.volume = Math.min(1, Math.max(0, left / fadeMs));
      this.sleep.timer = setTimeout(step, left > fadeMs ? left - fadeMs : 100);
    };
    step();
  }

  sleepEndOfSong() {
    this.setSleep({ kind: "song" });
  }

  sleepSongs(n) {
    this.setSleep(n <= 1 ? { kind: "song" } : { kind: "songs", left: n });
  }

  sleepEndOfQueue() {
    this.setSleep({ kind: "queue" });
  }

  sleepAfter(song, title) {
    this.setSleep({ kind: "after", song, title });
  }

  cancelSleep() {
    clearTimeout(this.sleep.timer);
    for (const el of this.els) el.volume = 1;
    this.sleep = { mode: null };
    this.updateSleep();
  }

  setSleep(mode) {
    this.cancelSleep();
    this.sleep.mode = mode;
    this.updateSleep();
  }

  /** A song started (on its own or skipped to): one fewer to go, and maybe this is the last. */
  onNextSongSleep() {
    const m = this.sleep.mode;
    if (m?.kind === "songs")
      this.sleep.mode =
        m.left <= 2 ? { kind: "song" } : { kind: "songs", left: m.left - 1 };
  }

  /** Playback paused itself at the end of the last song. */
  pausedAtEndSleep() {
    if (this.sleep.mode && this.sleep.mode.kind !== "at") this.cancelSleep();
  }

  /** Tells the player to stop where the playing song ends, when it is the last one. */
  updateSleep() {
    const m = this.sleep.mode;
    const cur = this.current?.id;
    const stop =
      m?.kind === "after"
        ? cur === m.song
        : m?.kind === "song"
          ? true
          : m?.kind === "queue"
            ? !this.hasNext
            : false;
    this.pauseAfter = stop ? cur : null;
    this.preloadNext();
    this.emit();
  }

  /** "in 23 min", "after this song", …, or null with no timer. */
  get sleepLabel() {
    const m = this.sleep.mode;
    if (!m) return null;
    switch (m.kind) {
      case "at":
        return `in ${Math.max(1, Math.ceil((m.end - nowMs()) / 60_000))} min`;
      case "song":
        return "after this song";
      case "songs":
        return `after ${m.left} songs`;
      case "queue":
        return "at the end of the queue";
      case "after":
        return `after “${m.title}”`;
    }
  }

  // MARK: The Media Session: media keys, the OS's now-playing panel, a phone's lock screen

  mediaSession() {
    const ms = navigator.mediaSession;
    if (!ms) return;
    const on = (action, f) => {
      try {
        ms.setActionHandler(action, f);
      } catch {
        // An action this browser does not know.
      }
    };
    on("play", () => this.play());
    on("pause", () => this.pause());
    on("previoustrack", () => this.previous());
    on("nexttrack", () => this.next());
    on("seekto", (d) => this.seek(d.seekTime));
    on("seekbackward", (d) => this.seek(this.position - (d.seekOffset ?? 10)));
    on("seekforward", (d) => this.seek(this.position + (d.seekOffset ?? 10)));
  }

  updateSession() {
    const ms = navigator.mediaSession;
    if (!ms) return;
    const s = this.current;
    if (!s) {
      ms.metadata = null;
      ms.playbackState = "none";
      return;
    }
    if (this.sessionSong !== s.id) {
      this.sessionSong = s.id;
      ms.metadata = new MediaMetadata({
        title: s.title,
        artist: s.displayArtist,
        album: s.album,
        artwork: s.art
          ? [256, 512].map((px) => ({
              src: new URL(this.app.cover(s, px), location.href).href,
              sizes: `${px}x${px}`,
              type: "image/jpeg",
            }))
          : [],
      });
    }
    ms.playbackState = this.isPlaying ? "playing" : "paused";
    this.updatePosition();
  }

  updatePosition() {
    const ms = navigator.mediaSession;
    if (!ms?.setPositionState || !this.current || !(this.duration > 0)) return;
    try {
      ms.setPositionState({
        duration: this.duration,
        playbackRate: this.speed,
        position: Math.min(this.position, this.duration),
      });
    } catch {
      // A position the browser finds out of range while a song opens.
    }
  }
}

const plural = (n) => (n === 1 ? "1 song" : `${n} songs`);
