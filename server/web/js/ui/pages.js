// The pages, one per address (docs/plans/030_web_client.md). Each returns
// {el, title, topics, update}: the shell calls `update` when a topic it
// lists changes, and the page redraws what that touches, keeping its
// scroll, selection and filter.

import { h, clock, count, summary, shuffled } from "../util.js";
import { icon } from "../icons.js";
import { allSongs, albumOrder, lists, listSongs, sorts } from "../catalog.js";
import { Query, nameIndex } from "../search.js";
import {
  VList,
  iconButton,
  menuAt,
  showMenu,
  prompt,
  confirm,
  emptyNote,
} from "./kit.js";
import {
  SongList,
  items,
  thumb,
  cover,
  editable,
  reserved,
  songMenu,
} from "./songs.js";
import { ctx } from "./ctx.js";

/** Pages get a filter only past this many rows: a shorter list fits on about one screen (plan 011). */
const filterOver = 12;

/** A page's frame: its bar (Back, the title, the filter), then its body. */
export function frame({ title, back = false, filter, actions = [], body, cls = "" }) {
  const bar = h(
    "header.pagebar",
    back ? iconButton("back", "Back", () => ctx.nav.back(), "back") : null,
    h("h1.pagetitle", title),
    h("span.spacer"),
    filter ? h("label.filter", icon("search"), filter) : null,
    ...actions,
  );
  const page = h("section.page", { class: cls }, bar, body);
  // The bar's title shows once the list's own big title has scrolled away.
  queueMicrotask(() => {
    const big = body.querySelector?.(".listtitle");
    if (!big) return;
    bar.classList.add("header-shown");
    new IntersectionObserver(
      ([e]) => bar.classList.toggle("header-shown", e.isIntersecting),
      { root: body },
    ).observe(big);
  });
  return page;
}

/** A filter box; `onInput` runs as it changes. Esc empties it. */
export function filterBox(prompt, onInput) {
  const input = h("input", {
    type: "search",
    placeholder: prompt,
    "aria-label": prompt,
    autocomplete: "off",
    spellcheck: false,
  });
  input.addEventListener("input", () => onInput(input.value));
  input.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && input.value) {
      e.stopPropagation();
      input.value = "";
      onInput("");
    }
  });
  return input;
}

/** A list's own header: its cover, name, a line under it, Play and Shuffle, and extra buttons. */
function listHeader({
  title,
  subtitle,
  songs,
  name = title,
  source,
  song,
  extra = [],
}) {
  const play = h(
    "button.primary",
    { type: "button", onclick: () => ctx.playback.playList(name, source, songs(), 0) },
    icon("play"),
    "Play",
  );
  const shuffle = h(
    "button",
    {
      type: "button",
      onclick: () => ctx.playback.playList(name, source, shuffled(songs()), 0),
    },
    icon("shuffle"),
    "Shuffle",
  );
  const sub = h("p.listsub", subtitle);
  const buttons = h("div.listbuttons", play, shuffle, ...extra);
  const el = h(
    "div.listhead",
    song === undefined ? null : cover(song, 512, "cover big"),
    h("div.listinfo", h("h2.listtitle", title), sub, buttons),
  );
  return {
    el,
    update(subtitle, has) {
      sub.textContent = subtitle;
      // Only when there is something to play, as on the phone.
      play.hidden = shuffle.hidden = !has;
    },
  };
}

/**
 * A page of songs with a header: an album, an artist, a genre, a list.
 * `get()` returns {title, subtitle, songs, song} as they are now, or null when gone.
 */
function songsPage({
  get,
  source,
  topics,
  track = false,
  album = true,
  extra,
  remove,
  strip,
  emptyText = "Nothing here",
}) {
  let data = get();
  if (!data) return missing();
  let query = "";
  const header = listHeader({
    title: data.title,
    subtitle: "",
    songs: () => data.songs,
    source,
    song: data.song ?? data.songs[0] ?? null,
    extra: data.extra ?? [],
  });
  const filter = filterBox("Filter songs", (q) => {
    query = q;
    show();
  });
  const list = new SongList({
    head: [header.el, strip?.el],
    name: data.title,
    source,
    track,
    album,
    extra,
    remove,
    empty: emptyNote(emptyText),
  });
  const show = () => {
    const all = items(data.songs);
    list.set(all, query ? items(ctx.app.catalog.filter(data.songs, query)) : all);
    header.update(data.subtitle ?? summary(data.songs), data.songs.length > 0);
    filter.parentElement &&
      (filter.parentElement.hidden = data.songs.length <= filterOver && !query);
    strip?.update();
  };
  const el = frame({ title: data.title, back: true, filter, body: list.el });
  show();
  return {
    el,
    title: data.title,
    topics: new Set(["catalog", "thumbs", "playback", ...(topics ?? [])]),
    update(t) {
      if (t.has("playback") || t.has("thumbs")) list.refresh();
      if ([...t].some((x) => x !== "playback" && x !== "thumbs")) {
        const next = get();
        if (!next) return ctx.nav.replace(ctx.nav.section());
        data = next;
        show();
      }
    },
    focus: () => list.el.focus({ preventScroll: true }),
  };
}

/**
 * A page whose subject is not here. It may only not have arrived yet (a
 * reload on an album's address, before the library is drawn), so the page
 * is made again when the library, the playlists or the queues change.
 */
function missing(text = "No longer in the library") {
  let first = true;
  return {
    el: frame({ title: "", back: true, body: emptyNote(text) }),
    title: "",
    topics: new Set(["catalog", "playlists", "queues"]),
    update() {
      // The shell's first call is the page being drawn; later ones are news.
      if (first) first = false;
      else ctx.shell.reopen();
    },
  };
}

// MARK: Albums

export function albumsPage() {
  let query = "";
  const grid = h("div.grid");
  const empty = h("div");
  const filter = filterBox("Filter albums", (q) => {
    query = q;
    draw();
  });
  const body = h("div.scroll", grid, empty);
  const draw = () => {
    const c = ctx.app.catalog;
    const shown = query ? c.index("albums").filter(new Query(query)) : c.albums;
    grid.replaceChildren(...shown.map(albumCard));
    empty.replaceChildren(
      c.albums.length === 0
        ? loading()
        : shown.length === 0
          ? emptyNote(`Nothing matches “${query}”`, null, "search")
          : "",
    );
  };
  draw();
  return {
    el: frame({ title: "Albums", filter, body }),
    title: "Albums",
    topics: new Set(["catalog"]),
    update: draw,
    scroller: body,
  };
}

function albumCard(a) {
  const card = h(
    "a.card",
    { href: ctx.nav.href("album", a.id), draggable: "false" },
    cover(a.songs[0], 256, "cover"),
    h("span.cardtitle", a.name),
    h("span.cardsub", a.artist),
  );
  card.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    showMenu(
      songMenu(a.songs, {
        play: () => ctx.playback.playList(a.name, `album:${a.id}`, a.songs, 0),
      }),
      { x: e.clientX, y: e.clientY },
      a.name,
    );
  });
  return card;
}

/** Shown while the library is still arriving. */
function loading() {
  return ctx.app.syncing || !ctx.app.catalog.songs.length
    ? h(
        "div.empty",
        h("div.spinner"),
        h("p.empty-title", ctx.app.syncing ? "Fetching the library…" : "Nothing here"),
      )
    : emptyNote("Nothing here");
}

export function albumPage(id) {
  return songsPage({
    get: () => {
      const a = ctx.app.catalog.albumById.get(id);
      if (!a) return null;
      return {
        title: a.name,
        subtitle: [a.artist, a.year > 0 ? String(a.year) : "", summary(a.songs)]
          .filter(Boolean)
          .join(" · "),
        songs: a.songs,
        song: a.songs[0],
        extra: [artistLink(a.artist)],
      };
    },
    source: `album:${id}`,
    track: true,
    album: false,
  });
}

function artistLink(name) {
  return ctx.app.catalog.artist(name)
    ? h("a.button", { href: ctx.nav.href("artist", name) }, icon("artist"), name)
    : null;
}

// MARK: Artists and genres

/** Artists or genres: a filtered list, each opening its songs. */
export function groupsPage(kind) {
  const title = kind === "artist" ? "Artists" : "Genres";
  let query = "";
  const list = new VList({
    cls: "groups",
    render: (g) => groupRow(kind, g),
    empty: h("div"),
  });
  const filter = filterBox(`Filter ${title.toLowerCase()}`, (q) => {
    query = q;
    draw();
  });
  const draw = () => {
    const c = ctx.app.catalog;
    const all = kind === "artist" ? c.artists : c.genres;
    const shown = query
      ? c.index(kind === "artist" ? "artists" : "genres").filter(new Query(query))
      : all;
    list.emptyEl.replaceChildren(
      all.length === 0
        ? loading()
        : shown.length === 0
          ? emptyNote(`Nothing matches “${query}”`, null, "search")
          : "",
    );
    list.setItems(shown);
  };
  list.el.append(list.emptyEl);
  draw();
  return {
    el: frame({ title, filter, body: list.el }),
    title,
    topics: new Set(["catalog", "thumbs"]),
    update: draw,
    scroller: list.el,
  };
}

function groupRow(kind, g) {
  const source = `${kind}:${g.name}`;
  const row = h(
    "a.row.group",
    { href: ctx.nav.href(kind, g.name) },
    thumb(g.songs[0]),
    h("span.main", h("span.title", g.name), h("span.sub", count(g.songs.length))),
    h(
      "button.more",
      {
        type: "button",
        title: "More",
        "aria-label": "More",
        onclick: (e) => {
          e.preventDefault();
          menuAt(e.currentTarget, groupMenu(g, source), g.name);
        },
      },
      icon("more"),
    ),
  );
  row.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    showMenu(groupMenu(g, source), { x: e.clientX, y: e.clientY }, g.name);
  });
  return row;
}

const groupMenu = (g, source) =>
  songMenu(albumOrder(g.songs), {
    play: () => ctx.playback.playList(g.name, source, albumOrder(g.songs), 0),
  });

export function artistPage(name) {
  const strip = {
    el: h("div.strip"),
    update() {
      const g = ctx.app.catalog.artist(name);
      if (!g) return;
      const ids = new Set(g.songs.map((s) => s.id));
      const albums = ctx.app.catalog.albums.filter((a) =>
        a.songs.some((s) => ids.has(s.id)),
      );
      this.el.hidden = albums.length < 2;
      this.el.replaceChildren(
        ...albums.map((a) =>
          h(
            "a.card.small",
            { href: ctx.nav.href("album", a.id) },
            cover(a.songs[0], 256),
            h("span.cardtitle", a.name),
          ),
        ),
      );
    },
  };
  return songsPage({
    get: () => {
      const g = ctx.app.catalog.artist(name);
      if (!g) return null;
      const songs = albumOrder(g.songs);
      const albums = new Set(
        songs.map((s) => ctx.app.catalog.albumOf.get(s.id)).filter(Boolean),
      ).size;
      return {
        title: g.name,
        subtitle: `${count(albums, "album")} · ${summary(songs)}`,
        songs,
        song: songs[0],
      };
    },
    source: `artist:${name}`,
    strip,
  });
}

export function genrePage(name) {
  return songsPage({
    get: () => {
      const g = ctx.app.catalog.genre(name);
      return g && { title: g.name, songs: albumOrder(g.songs), song: g.songs[0] };
    },
    source: `genre:${name}`,
  });
}

// MARK: Folders

/** The real NAS folder tree: subfolders, then the folder's own songs. */
export function folderPage(path) {
  const f0 = ctx.app.catalog.folders.get(path);
  if (!f0 && ctx.app.catalog.songs.length) return missing();
  let query = "";
  const top = path === "";
  const name = top ? "Folders" : (f0?.name ?? "");
  const header = top
    ? null
    : listHeader({
        title: name,
        subtitle: "",
        songs: () => allSongs(ctx.app.catalog.folders.get(path)),
        source: `folder:${path}`,
        song: null,
      });
  const folders = h("div.folders");
  const filter = filterBox("Filter folder", (q) => {
    query = q;
    show();
  });
  const list = new SongList({
    head: [header?.el, folders],
    name,
    source: `folder:${path}`,
    empty: h("div"),
  });
  const show = () => {
    const f = ctx.app.catalog.folders.get(path);
    if (!f) {
      list.o.empty.replaceChildren(loading());
      return;
    }
    const children = query
      ? nameIndex(f.children, "folder", (c) => c.name).filter(new Query(query))
      : f.children;
    folders.replaceChildren(...children.map(folderRow));
    const all = items(f.songs);
    list.set(all, query ? items(ctx.app.catalog.filter(f.songs, query)) : all);
    header?.update(summary(allSongs(f)), true);
    filter.parentElement &&
      (filter.parentElement.hidden =
        f.children.length + f.songs.length <= filterOver && !query);
    list.o.empty.replaceChildren(
      children.length || list.items.length
        ? ""
        : query
          ? emptyNote(`Nothing matches “${query}”`, null, "search")
          : loading(),
    );
  };
  const el = frame({ title: name, back: !top, filter, body: list.el });
  show();
  return {
    el,
    title: name,
    topics: new Set(["catalog", "thumbs", "playback"]),
    update(t) {
      if (t.has("catalog")) show();
      else list.refresh();
    },
    scroller: list.el,
  };
}

function folderRow(f) {
  const songs = () => allSongs(f);
  const menu = () =>
    songMenu(songs(), {
      play: () => ctx.playback.playList(f.name, `folder:${f.path}`, songs(), 0),
    });
  const row = h(
    "a.row.folder",
    { href: ctx.nav.href("folder", f.path) },
    h("span.thumb.folder-icon", icon("folder")),
    h("span.main", h("span.title", f.name), h("span.sub", count(songs().length))),
    h(
      "button.more",
      {
        type: "button",
        title: "More",
        "aria-label": "More",
        onclick: (e) => (e.preventDefault(), menuAt(e.currentTarget, menu(), f.name)),
      },
      icon("more"),
    ),
  );
  row.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    showMenu(menu(), { x: e.clientX, y: e.clientY }, f.name);
  });
  return row;
}

// MARK: Favorites, Listen Later and the automatic views (plan 010)

export function listPage(id) {
  const kind = lists.find((l) => l.id === id);
  if (!kind) return missing("No such list");
  return songsPage({
    get: () => ({ title: kind.title, songs: listSongs(id, ctx.app), song: null }),
    source: kind.source,
    topics: ["marks", "resumes", "stats"],
    extra: kind.mark
      ? (chosen) => [
          {
            label: `Remove from ${kind.title}`,
            action: () =>
              chosen.forEach((c) => ctx.app.mark(kind.mark, c.song.id, false)),
          },
        ]
      : undefined,
    remove: kind.mark
      ? (chosen) => chosen.forEach((c) => ctx.app.mark(kind.mark, c.song.id, false))
      : undefined,
    emptyText: kind.mark ? `Nothing in ${kind.title} yet` : "Nothing here yet",
  });
}

/** The phone's Playlists tab: the two lists and the automatic views as cards, then the playlists. */
export function playlistsPage() {
  const body = h("div.scroll");
  const draw = () => {
    const app = ctx.app;
    body.replaceChildren(
      h(
        "div.cards",
        lists.map((l) =>
          h(
            "a.listcard",
            { href: ctx.nav.href("list", l.id) },
            icon(l.icon),
            h("span", l.title),
            h("small", count(listSongs(l.id, app).length)),
          ),
        ),
      ),
      h("div.rows", app.playlists.map(playlistRow)),
      h(
        "button.newplaylist",
        { type: "button", onclick: newPlaylist },
        icon("add"),
        "New playlist",
      ),
    );
  };
  draw();
  return {
    el: frame({ title: "Playlists", body }),
    title: "Playlists",
    topics: new Set(["playlists", "marks", "resumes", "stats", "catalog"]),
    update: draw,
    scroller: body,
  };
}

export async function newPlaylist() {
  const n = await prompt("New playlist", "", {
    action: "Create",
    check: (n) => (reserved(n) ? `“${n}” is the name of one of your lists.` : ""),
  });
  if (n) ctx.nav.go("playlist", ctx.app.createPlaylist(n, []).id);
}

function playlistRow(p) {
  return h(
    "a.row.group",
    { href: ctx.nav.href("playlist", p.id) },
    h("span.thumb.folder-icon", icon(p.shared ? "shared" : "playlist")),
    h(
      "span.main",
      h("span.title", p.name),
      h(
        "span.sub",
        (p.shared ? "Shared · " : "") + count(ctx.app.catalog.songsOf(p.songs).length),
      ),
    ),
  );
}

// MARK: A playlist (plan 013)

/** The user's own to edit; a shared one is read-only. Entries that match no song are kept, shown as unavailable. */
export function playlistPage(id0) {
  const id = () => ctx.app.replaced.get(id0) ?? id0;
  const p0 = ctx.app.playlist(id());
  if (!p0) return missing("This playlist was deleted");
  let query = "";
  const app = ctx.app;
  const edit = h("button", { type: "button" }, icon("edit"), "Edit");
  edit.addEventListener("click", () => {
    const p = app.playlist(id());
    const songs = app.catalog.songsOf(p.songs);
    menuAt(edit, [
      {
        label: "Rename…",
        action: async () => {
          const n = await prompt("Rename playlist", p.name, {
            action: "Rename",
            check: (n) =>
              reserved(n) ? `“${n}” is the name of one of your lists.` : "",
          });
          if (n) app.renamePlaylist(app.playlist(id()), n);
        },
      },
      {
        label: "Sort",
        items: sorts.map(([label, f]) => ({
          label,
          action: () =>
            app.replacePlaylist(p, [
              ...f(songs).map((s) => s.id),
              ...p.songs.filter((x) => !app.catalog.byId.has(x)),
            ]),
        })),
      },
      "-",
      {
        label: "Delete playlist…",
        danger: true,
        action: async () => {
          if (
            await confirm(
              `Delete “${p.name}”?`,
              "The server keeps a copy of the file.",
              "Delete",
            )
          ) {
            app.deletePlaylist(app.playlist(id()));
            ctx.nav.replace("playlists");
          }
        },
      },
    ]);
  });
  const header = listHeader({
    title: p0.name,
    subtitle: "",
    songs: () => app.catalog.songsOf(app.playlist(id())?.songs ?? []),
    name: p0.name,
    source: `playlist:${p0.id}`,
    song: app.catalog.songsOf(p0.songs)[0] ?? null,
    extra: editable(p0) ? [edit] : [],
  });
  const remove = (chosen) => {
    // From the end, so the indexes still point at the right entries.
    for (const c of chosen.slice().sort((a, b) => b.index - a.index))
      app.removeFromPlaylist(app.playlist(id()), c.index);
  };
  const list = new SongList({
    head: header.el,
    name: p0.name,
    source: `playlist:${p0.id}`,
    reorder: editable(p0)
      ? (from, to) => !query && app.moveInPlaylist(app.playlist(id()), from, to)
      : null,
    extra: editable(p0)
      ? (chosen) => [{ label: "Remove from playlist", action: () => remove(chosen) }]
      : undefined,
    remove: editable(p0) ? remove : undefined,
    empty: emptyNote(
      "This playlist is empty",
      "Add songs from any list’s menu, or drag them onto it in the sidebar.",
    ),
  });
  const filter = filterBox("Filter songs", (q) => {
    query = q;
    show();
  });
  const show = () => {
    const p = app.playlist(id());
    if (!p) return ctx.nav.replace("playlists");
    const all = p.songs.map((sid, index) => ({
      song: app.catalog.byId.get(sid) ?? null,
      key: String(index),
      index,
    }));
    const songs = all.filter((it) => it.song);
    let shown = all;
    if (query) {
      const found = new Set(
        app.catalog.filter(
          songs.map((it) => it.song),
          query,
        ),
      );
      shown = all.filter((it) => it.song && found.has(it.song));
    }
    // Not while filtered: moving a song among the matches would put it in the wrong place.
    list.el.classList.toggle("no-reorder", !!query);
    list.set(all, shown);
    header.update(
      (p.shared ? "Shared · " : "") + summary(songs.map((it) => it.song)),
      songs.length > 0,
    );
    filter.parentElement &&
      (filter.parentElement.hidden = p.songs.length <= filterOver && !query);
    titleEl.textContent = p.name;
    header.el.querySelector(".listtitle").textContent = p.name;
  };
  const el = frame({ title: p0.name, back: true, filter, body: list.el });
  const titleEl = el.querySelector(".pagetitle");
  show();
  return {
    el,
    title: p0.name,
    topics: new Set(["playlists", "catalog", "thumbs", "playback"]),
    update(t) {
      if (t.has("playlists") || t.has("catalog")) show();
      else list.refresh();
    },
    focus: () => list.el.focus({ preventScroll: true }),
  };
}

// MARK: Queues (plan 020)

/** Numbered from 1 in the user's order and dragged to reorder; a click opens the queue's songs. */
export function queuesPage() {
  const list = new VList({
    cls: "queues",
    render: (q, i) => queueRow(q, i, list),
    reorder: (from, to) => ctx.playback.moveQueue(from, to),
    empty: emptyNote(
      "No queues",
      "Playing from any list starts a new queue; up to 20 are kept.",
      "queues",
    ),
  });
  list.el.append(list.emptyEl);
  const draw = () => list.setItems(ctx.playback.ordered);
  draw();
  return {
    el: frame({ title: "Queues", body: list.el }),
    title: "Queues",
    topics: new Set(["queues", "settings", "playback"]),
    update: draw,
    scroller: list.el,
  };
}

function queueRow(q, i, list) {
  const p = ctx.playback;
  const active = q.id === p.activeId;
  const row = h(
    "a.row.queue",
    { href: ctx.nav.href("queue", q.id), class: active ? "playing" : "" },
    h(
      "span.handle",
      {
        title: "Drag to move",
        onpointerdown: (e) => (e.preventDefault(), list.startDrag(e, i)),
        onclick: (e) => e.preventDefault(),
      },
      icon("drag"),
    ),
    h("span.num", String(i + 1)),
    h("span.main", h("span.title", q.name), h("span.sub", count(q.songs.length))),
    active
      ? h("span.eq", { class: p.isPlaying ? "on" : "" }, h("i"), h("i"), h("i"))
      : null,
    h(
      "button.more",
      {
        type: "button",
        title: "More",
        "aria-label": "More",
        onclick: (e) => (
          e.preventDefault(),
          menuAt(e.currentTarget, queueMenu(q), q.name)
        ),
      },
      icon("more"),
    ),
  );
  row.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    showMenu(queueMenu(q), { x: e.clientX, y: e.clientY }, q.name);
  });
  return row;
}

export function queueMenu(q) {
  const p = ctx.playback;
  const app = ctx.app;
  return [
    {
      label: q.id === p.activeId ? "Play" : "Switch to this queue",
      action: () => p.switchTo(q.id),
    },
    {
      label: "Rename…",
      action: async () => {
        const n = await prompt("Rename queue", q.name, {
          action: "Rename",
          check: (n) => (reserved(n) ? `“${n}” is the name of one of your lists.` : ""),
        });
        if (n) p.rename(q.id, n);
      },
    },
    {
      label: "Save as playlist…",
      action: async () => {
        const n = await prompt("Save as playlist", q.name, {
          action: "Save",
          check: (n) => (reserved(n) ? `“${n}” is the name of one of your lists.` : ""),
        });
        if (n) app.createPlaylist(n, q.id === p.activeId ? p.items : q.songs);
      },
    },
    "-",
    {
      label: "Remove queue…",
      danger: true,
      action: async () => {
        if (
          await confirm(
            `Remove “${q.name}”?`,
            "The queue goes; its songs stay in your library.",
            "Remove",
          )
        ) {
          p.delete(q.id);
          if (ctx.nav.route.name === "queue" && ctx.nav.route.arg === q.id)
            ctx.nav.replace("queues");
        }
      },
    },
    {
      label: "Remove all other queues",
      danger: true,
      action: async () => {
        if (await confirm(`Remove every queue but “${q.name}”?`, "", "Remove"))
          p.deleteOthers(q.id);
      },
    },
  ];
}

/**
 * A queue's songs: Play or Resume, Sort, the queue's menu, and "3 / 40 · 1:02
 * left of 2:30", which finds the current song. Used as a page and in the
 * window's right-hand panel (`compact`).
 */
export function queuePage(id, { compact = false } = {}) {
  if (!ctx.app.queue(id)) return missing("This queue was removed");
  const p = ctx.playback;
  const app = ctx.app;
  let query = "";
  const q = () => app.queue(id);
  const isActive = () => id === p.activeId;
  const ids = () => (isActive() ? p.items : (q()?.songs ?? []));
  const currentId = () => (isActive() ? p.current?.id : q()?.currentSong);
  const place = h("button.place", { type: "button", title: "Show the current song" });
  const playBtn = h("button.primary", { type: "button" });
  const sortBtn = h("button", { type: "button" }, icon("sort"), "Sort");
  const name = h("h2.queuename");
  const more = iconButton("more", "Queue menu", (e) =>
    menuAt(e.currentTarget, queueMenu(q()), q().name),
  );
  playBtn.addEventListener("click", () => (isActive() ? p.toggle() : p.switchTo(id)));
  sortBtn.addEventListener("click", () => {
    const songs = app.catalog.songsOf(ids());
    menuAt(
      sortBtn,
      sorts.map(([label, f]) => ({
        label,
        action: () =>
          p.reorder(
            id,
            f(songs).map((s) => s.id),
          ),
      })),
    );
  });
  const filter = filterBox("Filter queue", (v) => {
    query = v;
    show();
  });
  const head = h(
    "div.queuehead",
    compact ? h("div.queuetop", h("div", name, place), more) : h("div", place),
    h(
      "div.queuebuttons",
      playBtn,
      sortBtn,
      compact ? h("label.filter", icon("search"), filter) : null,
    ),
  );
  const remove = (chosen) => p.removeFrom(id, new Set(chosen.map((c) => c.song.id)));
  const list = new SongList({
    head,
    numbered: true,
    album: false,
    current: currentId,
    reorder: (from, to) => !query && p.moveIn(id, from, to),
    play: (it) => {
      if (isActive()) p.playAt(p.items.indexOf(it.song.id));
      else {
        app.setCurrent({ ...q(), currentSong: it.song.id, positionMs: 0 });
        p.switchTo(id);
      }
    },
    extra: (chosen) => {
      const s = chosen.length === 1 && chosen[0].song;
      const m = p.sleep.mode;
      return [
        s && isActive()
          ? m?.kind === "after" && m.song === s.id
            ? { label: "Don’t stop after this song", action: () => p.cancelSleep() }
            : {
                label: "Stop after this song",
                action: () => p.sleepAfter(s.id, s.title),
              }
          : null,
        { label: "Remove from queue", action: () => remove(chosen) },
      ].filter(Boolean);
    },
    remove,
    empty: emptyNote("This queue is empty"),
  });
  place.addEventListener("click", () => {
    const i = list.items.findIndex((it) => it.song?.id === currentId());
    if (i >= 0) list.list.scrollTo(i);
  });
  let lastIds = null;
  const show = () => {
    if (!q()) return compact ? null : ctx.nav.replace("queues");
    const songs = app.catalog.songsOf(ids());
    const all = items(songs);
    list.el.classList.toggle("no-reorder", !!query);
    list.set(all, query ? items(app.catalog.filter(songs, query)) : all);
    name.textContent = q().name;
    titleEl && (titleEl.textContent = q().name);
    filter.parentElement &&
      !compact &&
      (filter.parentElement.hidden = songs.length <= filterOver && !query);
    sortBtn.disabled = songs.length < 2;
    showPlace(songs);
    lastIds = ids();
  };
  const showPlace = (songs = app.catalog.songsOf(ids())) => {
    const total = songs.reduce((a, s) => a + s.durationMs, 0);
    const i = songs.findIndex((s) => s.id === currentId());
    if (i < 0) place.textContent = summary(songs);
    else {
      const into = isActive() ? p.position * 1000 : q().positionMs;
      const left = songs.slice(i).reduce((a, s) => a + s.durationMs, 0) - into;
      place.textContent = `${i + 1} / ${songs.length} · ${clock(left / 1000)} left of ${clock(total / 1000)}`;
    }
    playBtn.replaceChildren(
      ...(isActive()
        ? [icon(p.isPlaying ? "pause" : "play"), p.isPlaying ? "Pause" : "Play"]
        : [icon("play"), "Resume"]),
    );
  };
  const el = compact
    ? h("section.page.compact", list.el)
    : frame({ title: q().name, back: true, filter, body: list.el, actions: [more] });
  const titleEl = compact ? null : el.querySelector(".pagetitle");
  show();
  // The current song in view when the queue opens.
  requestAnimationFrame(() => {
    const i = list.items.findIndex((it) => it.song?.id === currentId());
    if (i >= 0) list.list.scrollTo(i);
  });
  return {
    el,
    title: q().name,
    topics: new Set(["queues", "catalog", "thumbs", "playback", "position"]),
    update(t) {
      if (
        t.has("queues") ||
        t.has("catalog") ||
        (t.has("playback") && ids() !== lastIds)
      )
        show();
      else if (t.has("playback") || t.has("thumbs")) {
        list.refresh();
        showPlace();
      } else if (t.has("position")) showPlace();
    },
    focus: () => list.el.focus({ preventScroll: true }),
  };
}
