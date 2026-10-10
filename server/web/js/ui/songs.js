// Song rows and what they do: the list used by every page that shows songs,
// the phone's song menu, and covers.

import { h, clock, count } from "../util.js";
import { icon } from "../icons.js";
import { VList, showMenu, prompt, touch, toast } from "./kit.js";
import { ctx } from "./ctx.js";

/** A list row's cover: the thumbnail every cover has, never waiting (plan 019). */
export function thumb(s, cls = "thumb") {
  const src = ctx.app.thumb(s);
  if (src)
    return h("img", { class: cls, src, alt: "", loading: "lazy", decoding: "async" });
  return h("span", { class: cls + " placeholder" }, icon("note"));
}

/**
 * A larger cover: the thumbnail at once, then the cover itself fades in
 * over it when it arrives, so a page never shows an empty square first.
 * Lazily: the albums grid holds every album, thousands of covers, and only
 * those near the screen should be fetched.
 */
export function cover(s, px, cls = "cover") {
  const el = h("span", { class: cls });
  const t = s && ctx.app.thumb(s);
  if (t) el.append(h("img.under", { src: t, alt: "" }));
  else el.append(h("span.placeholder", icon("note")));
  const url = s && ctx.app.cover(s, px);
  if (url) {
    const img = h("img.over", { alt: "", decoding: "async", loading: "lazy" });
    img.addEventListener("load", () => img.classList.add("loaded"));
    img.addEventListener("error", () => img.remove());
    img.src = url;
    el.append(img);
  }
  return el;
}

/** Shared playlists stay read-only in the app, for every user (plan 013). */
export const editable = (p) => !p.shared;

/** The two lists' names cannot name a queue or playlist, as on the phone. */
export const reserved = (name) =>
  ["favorites", "listen later"].includes(name.toLowerCase());
const checkName = (n) =>
  reserved(n) ? `“${n}” is the name of one of your lists.` : "";

/**
 * The phone's song menu (plan 011): play next, add to the playing queue or
 * another, add to a playlist, the two lists, go to album or artist. `play`
 * plays the list from the first chosen song.
 */
export function songMenu(songs, { play, extra = [] } = {}) {
  const { app, playback, nav } = ctx;
  if (!songs.length) return [];
  const allFav = songs.every((s) => app.isMarked("fav", s.id));
  const allLater = songs.every((s) => app.isMarked("later", s.id));
  const one = songs.length === 1 ? songs[0] : null;
  return [
    play ? { label: "Play", action: play } : null,
    { label: "Play next", action: () => playback.playNext(songs) },
    { label: "Add to playing queue", action: () => playback.addToQueue(songs) },
    {
      label: "Add to queue",
      items: [
        ...playback.ordered.map((q, i) => ({
          label: `${i + 1}. ${q.name}`,
          action: () => playback.addTo(q.id, songs),
        })),
        playback.ordered.length ? "-" : null,
        {
          label: "New queue…",
          action: async () => {
            const n = await prompt("New queue", one?.title ?? "", {
              action: "Create",
              check: checkName,
            });
            if (n) playback.newQueue(n, songs);
          },
        },
      ],
    },
    {
      label: "Add to playlist",
      items: [
        ...app.playlists
          .filter(editable)
          .map((p) => ({ label: p.name, action: () => addedTo(p, songs) })),
        app.playlists.some(editable) ? "-" : null,
        {
          label: "New playlist…",
          action: async () => {
            const n = await prompt("New playlist", "", {
              action: "Create",
              check: checkName,
            });
            if (n) {
              app.createPlaylist(
                n,
                songs.map((s) => s.id),
              );
              toast(`Added ${count(songs.length)} to “${n}”`);
            }
          },
        },
      ],
    },
    "-",
    {
      label: allFav ? "Remove from Favorites" : "Add to Favorites",
      action: () => songs.forEach((s) => app.mark("fav", s.id, !allFav)),
    },
    {
      label: allLater ? "Remove from Listen Later" : "Listen Later",
      action: () => songs.forEach((s) => app.mark("later", s.id, !allLater)),
    },
    one ? "-" : null,
    one && one.album ? { label: "Go to album", action: () => nav.album(one) } : null,
    one ? { label: "Go to artist", action: () => nav.artist(one) } : null,
    one ? { label: "Song info", action: () => songInfo(one) } : null,
    ...(extra.length ? ["-", ...extra] : []),
  ].filter(Boolean);
}

/** Adds the songs and says what happened, as the phone does: songs already there are skipped. */
export function addedTo(p, songs) {
  const ids = songs.map((s) => s.id);
  const skipped = ctx.app.addToPlaylist(p, ids);
  const adding = new Set(ids).size - skipped;
  toast(
    adding === 0
      ? skipped === 1
        ? `Already in “${p.name}”`
        : `All ${skipped} already in “${p.name}”`
      : skipped > 0
        ? `Added ${adding} to “${p.name}” · ${skipped} already there`
        : `Added ${count(adding)} to “${p.name}”`,
  );
}

/** What the file is, as the phone's song details show. */
async function songInfo(s) {
  const { dialog } = await import("./kit.js");
  const rows = [
    ["Title", s.title],
    ["Artist", s.artist],
    ["Album", s.album],
    ["Album artist", s.albumArtist],
    ["Composer", s.composer],
    ["Genre", s.genres.join(", ")],
    ["Year", s.year || ""],
    ["Track", s.track ? (s.disc ? `${s.disc}.${s.track}` : s.track) : ""],
    ["Length", s.durationMs ? clock(s.durationMs / 1000) : ""],
    ["Format", [s.format, s.codec].filter(Boolean).join(" · ")],
    [
      "Quality",
      [
        s.bitrate && `${s.bitrate} kbit/s`,
        s.sampleRate && `${s.sampleRate / 1000} kHz`,
        s.bitDepth && `${s.bitDepth}-bit`,
      ]
        .filter(Boolean)
        .join(" · "),
    ],
    ["Size", s.size ? `${(s.size / 1e6).toFixed(1)} MB` : ""],
    ["File", s.path],
  ].filter(([, v]) => v !== "" && v != null);
  await dialog(
    s.title,
    h(
      "dl.info",
      rows.map(([k, v]) => [h("dt", k), h("dd", String(v))]),
    ),
    [{ label: "Close", submit: true, primary: true }],
    { cls: "wide" },
  );
}

/**
 * A list of songs. Double-click, Return or (on a touch screen) a tap plays
 * from that song in a new queue named after the list (AGENTS.md: never the
 * current queue); a click selects, ⌘ or Ctrl adds, Shift selects a run; a
 * right click or ⋯ opens the song menu. Rows can be dragged onto a playlist
 * in the sidebar, and with `reorder`, moved by their handle.
 *
 * Items are `{song, key}` (a playlist's unmatched entry has no song).
 * Options: play(item, index) (default: the whole list from that song, as
 * `name` and `source`), extra(items) → more menu items, remove(items) for
 * Delete, current() → the song id to mark, numbered, track, album.
 */
export class SongList {
  constructor(opts) {
    this.o = opts;
    this.all = [];
    this.items = [];
    this.selected = new Set();
    this.anchor = -1;
    this.list = new VList({
      head: opts.head,
      foot: opts.foot,
      cls:
        "songs" +
        (opts.track ? " with-track" : "") +
        (opts.album === false ? " no-album" : "") +
        (opts.numbered ? " numbered" : ""),
      render: (it, i) => this.row(it, i),
      reorder: opts.reorder,
      empty: opts.empty,
    });
    const el = (this.el = this.list.el);
    el.tabIndex = 0;
    el.setAttribute("role", "listbox");
    el.setAttribute("aria-multiselectable", "true");
    el.addEventListener("keydown", (e) => this.key(e));
    if (opts.empty) el.append(opts.empty);
  }

  /** `all` is what plays; `shown` (filtered) is what the rows are. */
  set(all, shown = all) {
    this.all = all;
    const keys = new Set(shown.map((it) => it.key));
    for (const k of this.selected) if (!keys.has(k)) this.selected.delete(k);
    this.items = shown;
    this.list.setItems(shown);
  }

  refresh() {
    this.list.paint(true);
  }

  row(it, i) {
    const s = it.song;
    const cur = this.o.current ? this.o.current() : ctx.playback.current?.id;
    const playing = s && cur === s.id;
    const el = h(
      "div.row",
      {
        role: "option",
        "aria-selected": String(this.selected.has(it.key)),
        class: [
          playing ? "playing" : "",
          this.selected.has(it.key) ? "selected" : "",
          s ? "" : "unavailable",
        ].join(" "),
        draggable: s && !touch() ? "true" : null,
      },
      this.o.numbered ? h("span.num", String(i + 1)) : null,
      this.o.reorder
        ? h(
            "span.handle",
            { title: "Drag to move", onpointerdown: (e) => this.list.startDrag(e, i) },
            icon("drag"),
          )
        : null,
      this.o.track ? h("span.num", s?.track ? String(s.track) : "") : null,
      s ? thumb(s) : h("span.thumb.placeholder", icon("note")),
      h(
        "span.main",
        h(
          "span.title",
          s ? s.title : "Unavailable",
          playing
            ? h(
                "span.eq",
                {
                  "aria-label": ctx.playback.isPlaying ? "Playing" : "Paused",
                  class: ctx.playback.isPlaying ? "on" : "",
                },
                h("i"),
                h("i"),
                h("i"),
              )
            : null,
        ),
        h(
          "span.sub",
          s
            ? [s.displayArtist, this.o.album === false ? "" : s.album]
                .filter(Boolean)
                .join(" · ")
            : "Not in the library",
        ),
      ),
      h("span.artist", s?.displayArtist ?? ""),
      this.o.album === false ? null : h("span.album", s?.album ?? ""),
      h("span.time", s?.durationMs ? clock(s.durationMs / 1000) : ""),
      h(
        "button.more",
        {
          type: "button",
          title: "More",
          "aria-label": "More",
          onclick: (e) => this.menuFor(i, e, true),
        },
        icon("more"),
      ),
    );
    el.addEventListener("click", (e) => this.click(e, i));
    el.addEventListener("dblclick", (e) => {
      if (!touch() && !e.target.closest("button,.handle")) this.play(i);
    });
    el.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      this.menuFor(i, e);
    });
    if (s && !touch())
      el.addEventListener("dragstart", (e) => {
        const chosen = this.selected.has(it.key) ? this.chosen() : [it];
        const ids = chosen.map((c) => c.song?.id).filter(Boolean);
        e.dataTransfer.setData("application/x-dhun-songs", JSON.stringify(ids));
        e.dataTransfer.setData(
          "text/plain",
          chosen.map((c) => c.song?.title).join("\n"),
        );
        e.dataTransfer.effectAllowed = "copy";
        document.body.classList.add("dragging-songs");
      });
    el.addEventListener("dragend", () =>
      document.body.classList.remove("dragging-songs"),
    );
    return el;
  }

  /** The selected items, in list order. */
  chosen() {
    return this.items.filter((it) => this.selected.has(it.key));
  }

  click(e, i) {
    if (e.target.closest("button,.handle")) return;
    if (touch()) {
      if (this.selected.size) this.toggle(i);
      else this.play(i);
      return;
    }
    const key = this.items[i].key;
    if (e.shiftKey && this.anchor >= 0) {
      const [a, b] = [Math.min(this.anchor, i), Math.max(this.anchor, i)];
      if (!e.metaKey && !e.ctrlKey) this.selected.clear();
      for (let j = a; j <= b; j++) this.selected.add(this.items[j].key);
    } else if (e.metaKey || e.ctrlKey) {
      this.toggle(i);
      return;
    } else {
      this.selected = new Set([key]);
      this.anchor = i;
    }
    this.refresh();
  }

  toggle(i) {
    const key = this.items[i].key;
    if (this.selected.has(key)) this.selected.delete(key);
    else this.selected.add(key);
    this.anchor = i;
    this.refresh();
  }

  play(i) {
    const it = this.items[i];
    if (!it?.song) return;
    if (this.o.play) return this.o.play(it, i);
    const songs = this.all.map((x) => x.song).filter(Boolean);
    const start = songs.indexOf(it.song);
    ctx.playback.playList(this.o.name, this.o.source, songs, Math.max(0, start));
    this.o.onPlayed?.();
  }

  menuFor(i, e, fromButton = false) {
    const it = this.items[i];
    if (!this.selected.has(it.key) || fromButton) {
      this.selected = new Set([it.key]);
      this.anchor = i;
      this.refresh();
    }
    const chosen = this.chosen();
    const songs = chosen.map((c) => c.song).filter(Boolean);
    const items = songMenu(songs, {
      play: songs.length === 1 && !this.o.noPlay ? () => this.play(i) : null,
      extra: this.o.extra?.(chosen) ?? [],
    });
    if (!items.length && this.o.extra) items.push(...this.o.extra(chosen));
    const title = songs.length === 1 ? songs[0].title : count(chosen.length);
    if (fromButton) {
      const r = e.currentTarget.getBoundingClientRect();
      showMenu(items, { x: r.right - 200, y: r.bottom }, title);
    } else showMenu(items, { x: e.clientX, y: e.clientY }, title);
  }

  key(e) {
    if (e.target !== this.el) return;
    const n = this.items.length;
    if (!n) return;
    const at = this.anchor < 0 ? -1 : this.anchor;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      const i = Math.max(0, Math.min(n - 1, at + (e.key === "ArrowDown" ? 1 : -1)));
      if (e.shiftKey) this.selected.add(this.items[i].key);
      else this.selected = new Set([this.items[i].key]);
      this.anchor = i;
      this.list.scrollTo(i, false);
      this.refresh();
    } else if (e.key === "Enter" && at >= 0) {
      this.play(this.items.findIndex((it) => this.selected.has(it.key)));
    } else if (
      (e.key === "Delete" || e.key === "Backspace") &&
      this.o.remove &&
      this.selected.size
    ) {
      this.o.remove(this.chosen());
    } else if (e.key === "a" && (e.metaKey || e.ctrlKey)) {
      this.selected = new Set(this.items.map((it) => it.key));
      this.refresh();
    } else if (e.key === "Escape" && this.selected.size) {
      this.selected.clear();
      this.refresh();
    } else return;
    e.preventDefault();
  }
}

/** Songs as list items, keyed by id. */
export const items = (songs) => songs.map((s) => ({ song: s, key: String(s.id) }));
