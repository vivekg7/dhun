// Audiobooks and podcasts (plan 031): every book or show, one's files, and
// the bookmarks (plan 033), chapters and notes of the file playing.

import { h, clock, count } from "../util.js";
import { icon } from "../icons.js";
import { chapterName, day } from "../catalog.js";
import { rowMenu, emptyNote, iconButton, dialog } from "./kit.js";
import { SongList, items, thumb, cover } from "./songs.js";
import { frame, filterBox, missing, namesPage, showFilter } from "./pages.js";
import { ctx } from "./ctx.js";
import { synced } from "../synced.js";

/**
 * Playing from a book makes a queue of the whole book, named after it, from
 * the file picked or the first one not played; its resume point does the
 * rest. Playing an episode makes a queue of the show's episodes from that one
 * on, those already played left out.
 */
export function playShelf(shelf, from) {
  const heard = ctx.app.heard;
  const start = from ?? heard.next(shelf);
  const source = (shelf.isBook ? "book:" : "show:") + shelf.title;
  const i = Math.max(0, shelf.items.indexOf(start));
  if (shelf.isBook) return ctx.playback.playList(shelf.title, source, shelf.items, i);
  const rest = shelf.items
    .slice(i)
    .filter((s) => s === start || !heard.played.has(s.id));
  ctx.playback.playList(shelf.title, source, rest, 0);
}

/** A book's or a show's line under its title, in lists. */
function detail(s, heard) {
  if (s.isBook) return [s.author, heard.progress(s)].filter(Boolean).join(" · ");
  const fresh = s.items.filter((x) => !heard.played.has(x.id)).length;
  return count(s.items.length, "episode") + (fresh ? ` · ${fresh} not played` : "");
}

/** Every book, or every show. */
export function shelvesPage(books) {
  return namesPage({
    title: books ? "Audiobooks" : "Podcasts",
    prompt: books ? "Filter books" : "Filter podcasts",
    all: (c) => (books ? c.books : c.shows),
    index: books ? "books" : "shows",
    row: shelfRow,
    empty: () => emptyNote("Nothing here yet", null, books ? "book" : "podcast"),
    topics: ["catalog", "thumbs", "marks", "resumes"],
  });
}

function shelfRow(s) {
  const heard = ctx.app.heard;
  const menu = () => [
    { label: heard.begun(s) ? "Continue" : "Play", action: () => playShelf(s) },
  ];
  const row = h(
    "a.row.group",
    { href: ctx.nav.href("shelf", s.id) },
    thumb(s.cover ?? s.items[0]),
    h("span.main", h("span.title", s.title), h("span.sub", detail(s, heard))),
  );
  return rowMenu(row, menu, s.title);
}

/** A book, its files in reading order; or a show, its episodes newest first, those played hidden unless asked for. */
export function shelfPage(id) {
  const app = ctx.app;
  const get = () => app.catalog.shelves.get(id);
  if (!get()) return missing();
  let query = "";
  const sub = h("p.listsub");
  const play = h("button.primary", { type: "button", onclick: () => playShelf(get()) });
  const hide = h("button", {
    type: "button",
    onclick: () => app.setSetting(synced.hidePlayed.name, !hidePlayed()),
  });
  const hidePlayed = () => !get().isBook && app.setting(synced.hidePlayed) !== false;
  const s0 = get();
  const header = h(
    "div.listhead",
    cover(s0.cover ?? s0.items[0], 512, "cover big"),
    h(
      "div.listinfo",
      h("h2.listtitle", s0.title),
      sub,
      h("div.listbuttons", play, s0.isBook ? null : hide),
    ),
  );
  // A book's files are often titled alike but for a part number at the end.
  let part = new Map();
  const list = new SongList({
    head: header,
    name: s0.title,
    album: false,
    play: (it) => playShelf(get(), it.song),
    detail: (s) => {
      const heard = app.heard;
      const where = get()?.isBook ? (part.get(s.id) ?? "") : day(s);
      return [where, heard.state(s)].filter(Boolean).join(" · ");
    },
    empty: h("div"),
  });
  const filter = filterBox(s0.isBook ? "Filter files" : "Filter episodes", (q) => {
    query = q;
    show();
  });
  const show = () => {
    const shelf = get();
    if (!shelf) return ctx.nav.replace(ctx.nav.section());
    const heard = app.heard;
    const hiding = hidePlayed();
    const all = shelf.isBook
      ? shelf.items
      : shelf.items
          .slice()
          .reverse()
          .filter((s) => !hiding || !heard.played.has(s.id));
    part = new Map(
      shelf.items.length > 1
        ? shelf.items.map((s, i) => [s.id, `${i + 1} of ${shelf.items.length}`])
        : [],
    );
    list.set(items(all), query ? items(app.catalog.filter(all, query)) : items(all));
    const fresh = shelf.items.filter((s) => !heard.played.has(s.id)).length;
    sub.textContent = shelf.isBook
      ? detail(shelf, heard)
      : `${count(shelf.items.length, "episode")} · ${fresh} not played`;
    play.replaceChildren(icon("play"), heard.begun(shelf) ? "Continue" : "Play");
    hide.replaceChildren(
      icon(hiding ? "eye" : "eyeOff"),
      hiding ? "Show played" : "Hide played",
    );
    showFilter(filter, all.length, query);
    list.o.empty.replaceChildren(
      list.items.length
        ? ""
        : query
          ? emptyNote(`Nothing matches “${query}”`, null, "search")
          : emptyNote("Every episode is played", null, "check"),
    );
  };
  const el = frame({ title: s0.title, back: true, filter, body: list.el });
  show();
  return {
    el,
    title: s0.title,
    topics: new Set(["catalog", "thumbs", "playback", "marks", "resumes", "settings"]),
    update(t) {
      if (["catalog", "marks", "resumes", "settings"].some((x) => t.has(x))) show();
      else list.refresh();
    },
    focus: () => list.el.focus({ preventScroll: true }),
  };
}

const chapterAt = (s, ms) => s.chapters.findLastIndex((c) => c.startMs <= ms);

/** The facts under an episode's or a book file's title: its show and day, or its book and author. */
export function spokenFacts(s) {
  if (s.kind === "podcast") return [s.groupTitle, day(s)];
  return [s.groupTitle === s.title ? "" : s.groupTitle, s.artist || s.albumArtist];
}

/**
 * The line under the title on Now playing that opens the chapters panel: the
 * chapter playing ("Chapter 2 of 20 · Name"), or, in a file without
 * chapters, how many bookmarks it has, so a long music file reaches them too
 * (plan 033). Null for neither.
 */
export function chapterLine(s, positionMs) {
  if (s.spoken && s.chapters.length) {
    const i = Math.max(0, chapterAt(s, positionMs));
    const name = chapterName(s.chapters[i]);
    return `Chapter ${i + 1} of ${s.chapters.length}` + (name ? ` · ${name}` : "");
  }
  const n = ctx.app.bookmarks(s.id).length;
  return n ? count(n, "bookmark") : null;
}

/** The server's limit on a bookmark's note, in characters. */
const maxNote = 1000;

/** An episode's or a book file's bookmarks and chapters, each a click away, and its notes. */
export function chaptersPanel(close) {
  const p = ctx.playback;
  const s = p.current;
  if (!s) return h("div.pop", h("p.note", "Nothing playing"));
  const el = h("div.pop.chapters");
  const chapterOf = (ms) => {
    const i = chapterAt(s, ms);
    return i < 0 ? "" : chapterName(s.chapters[i]) || `Chapter ${i + 1}`;
  };
  const play = (ms) => () => {
    p.seek(ms / 1000);
    close();
  };
  // A bookmark: its time, its note or else its chapter, and buttons to change the note or delete it.
  const mark = (b) => {
    const chapter = chapterOf(b.positionMs);
    return h(
      "div.bookmark",
      h(
        "button.plain.chapter",
        { type: "button", onclick: play(b.positionMs) },
        h("span.time", clock(b.positionMs / 1000)),
        h(
          "span.main",
          h("span.title", b.note || chapter || "Bookmark"),
          b.note && chapter ? h("span.sub", chapter) : null,
        ),
      ),
      iconButton("edit", b.note ? "Edit the note" : "Add a note", async () => {
        if (await editNote(b)) draw();
      }),
      iconButton("close", "Delete", () => {
        ctx.app.bookmark({ ...b, note: "", deleted: true });
        draw();
      }),
    );
  };
  const draw = () => {
    const marks = ctx.app.bookmarks(s.id);
    const now = chapterAt(s, p.position * 1000);
    const parts = [
      h("h3", s.title),
      marks.length ? [h("p.pophead", "Bookmarks"), marks.map(mark)] : null,
      s.chapters.length
        ? [
            h("p.pophead", "Chapters"),
            s.chapters.map((c, i) =>
              h(
                "button.plain.chapter",
                {
                  type: "button",
                  class: i === now ? "on" : "",
                  onclick: play(c.startMs),
                },
                h("span.time", clock(c.startMs / 1000)),
                h("span", chapterName(c) || `Chapter ${i + 1}`),
              ),
            ),
          ]
        : null,
      s.notes ? [h("p.pophead", "Notes"), h("p.notes", s.notes)] : null,
      !s.chapters.length && !s.notes && !marks.length
        ? h("p.note", "No chapters or notes")
        : null,
    ];
    el.replaceChildren(...parts.flat(2).filter(Boolean));
  };
  draw();
  // The chapter playing in view and focused, once the panel is placed, rather than the first.
  setTimeout(() => {
    const on = el.querySelector(".chapter.on");
    on?.focus({ preventScroll: true });
    on?.scrollIntoView({ block: "center" });
  });
  return el;
}

/** Writes or changes a bookmark's note; true when saved. */
async function editNote(b) {
  const input = h("textarea.notefield", {
    rows: 4,
    maxlength: maxNote,
    placeholder: "What happens here",
    "aria-label": "Note",
  });
  input.value = b.note;
  const r = await dialog(
    `Note at ${clock(b.positionMs / 1000)}`,
    [input],
    [{ label: "Cancel" }, { label: "Save", submit: true, primary: true, value: "ok" }],
    { onOpen: () => input.focus() },
  );
  if (r !== "ok") return false;
  ctx.app.bookmark({ ...b, note: input.value.trim() });
  return true;
}
