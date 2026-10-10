// What search finds (plan 029), as on the phone and the Mac: artists, albums,
// genres, folders, playlists, songs and lyrics, the section with the best
// match first. A song plays the songs found; anything else opens over the
// results, so Back returns to them.

import { h, count, debounce } from "../util.js";
import { icon } from "../icons.js";
import { Query, nameIndex, lyricsHits } from "../search.js";
import { emptyNote } from "./kit.js";
import { SongList, thumb, items } from "./songs.js";
import { frame } from "./pages.js";
import { ctx } from "./ctx.js";

/** Results a section shows before "Show all": one strong song should not sit under twenty albums. */
const shown = 3;
/** Songs found are many in a big library; past this many, the query is too loose to read through. */
const songsShown = 300;

/** Every section with something in it, the one with the best match first; on a tie, in this order. */
function find(q) {
  const { app } = ctx;
  const c = app.catalog;
  const favs = new Set(app.marked("fav"));
  const boost = (s) =>
    (favs.has(s.id) ? 3 : 0) +
    Math.min(2, Math.floor((app.playStats.get(s.id)?.count ?? 0) / 5));
  const section = (kind, index, row, b) => {
    const hits = index.search(q, b);
    if (!hits.length) return null;
    return {
      kind,
      rows: hits
        .slice(0, kind === "Songs" ? songsShown : hits.length)
        .map((x) => row(x.item)),
      best: hits[0].score,
    };
  };
  const found = [
    section("Artists", c.index("artists"), (g) => ({
      title: g.name,
      detail: count(g.songs.length),
      song: g.songs[0],
      href: ctx.nav.href("artist", g.name),
    })),
    section("Albums", c.index("albums"), (a) => ({
      title: a.name,
      detail: [a.artist, a.year > 0 ? a.year : ""].filter(Boolean).join(" · "),
      song: a.songs[0],
      href: ctx.nav.href("album", a.id),
    })),
    section("Genres", c.index("genres"), (g) => ({
      title: g.name,
      detail: count(g.songs.length),
      song: g.songs[0],
      href: ctx.nav.href("genre", g.name),
    })),
    // Many folders share a name ("CD1"), so the path says which.
    section("Folders", c.index("folders"), (f) => ({
      title: f.name,
      detail: f.path.includes("/")
        ? f.path.slice(0, f.path.lastIndexOf("/")).replaceAll("/", " › ")
        : "Music",
      icon: "folder",
      href: ctx.nav.href("folder", f.path),
    })),
    section(
      "Playlists",
      nameIndex(app.playlists, "playlist", (p) => p.name),
      (p) => ({
        title: p.name,
        detail: count(c.songsOf(p.songs).length),
        icon: p.shared ? "shared" : "playlist",
        href: ctx.nav.href("playlist", p.id),
      }),
    ),
    section("Songs", c.index("songs"), (s) => s, boost),
  ];
  // Stable, so a tie keeps the order above.
  return found
    .map((f, i) => f && { ...f, i })
    .filter(Boolean)
    .sort((a, b) => a.best - b.best || a.i - b.i);
}

/**
 * The results page. On a wide window the query comes from the search field
 * at the top; on a phone the page has its own. `query` is the address's.
 */
export function searchPage(initial, { field = false } = {}) {
  let query = initial;
  let expanded = new Set();
  let lyricsSeq = 0;
  const before = h("div.results");
  const songsHead = h("h3.section", "Songs");
  const after = h("div.results");
  const lyricsBox = h("div.results");
  const recent = h("div.recent");
  const list = new SongList({
    head: [recent, before, songsHead],
    foot: [after, lyricsBox],
    name: "",
    source: "search",
    onPlayed: () => ctx.app.keepSearch(query),
  });
  const input = field
    ? h("input.searchinput", {
        type: "search",
        value: initial,
        placeholder: "Songs, albums, artists, lyrics",
        "aria-label": "Search",
        autocomplete: "off",
        spellcheck: false,
      })
    : null;
  input?.addEventListener("input", () => {
    query = input.value;
    ctx.nav.replace("search", query);
    draw();
  });
  const empty = h("div");
  list.el.append(empty);

  const sectionEl = (f) => {
    const whole = expanded.has(f.kind);
    const rows = whole ? f.rows : f.rows.slice(0, shown);
    return h(
      "section.results-section",
      h("h3.section", f.kind),
      rows.map((r) =>
        h(
          "a.row.hit",
          { href: r.href, onclick: () => ctx.app.keepSearch(query) },
          r.icon ? h("span.thumb.folder-icon", icon(r.icon)) : thumb(r.song),
          h("span.main", h("span.title", r.title), h("span.sub", r.detail)),
        ),
      ),
      !whole && f.rows.length > shown
        ? h(
            "button.link",
            { type: "button", onclick: () => (expanded.add(f.kind), draw()) },
            `Show all ${f.rows.length} ${f.kind.toLowerCase()}`,
          )
        : null,
    );
  };

  const fetchLyrics = debounce(async (q, seq) => {
    try {
      const hits = await ctx.app.searchLyrics(q.lyrics);
      if (seq !== lyricsSeq) return;
      const songs = hits
        .map((x) => ({ song: ctx.app.catalog.byId.get(x.song), line: x.line }))
        .filter((x) => x.song);
      showLyrics(songs, q);
    } catch {
      if (seq === lyricsSeq) lyricsBox.replaceChildren();
    }
  }, 400);

  const showLyrics = (hits) => {
    if (!hits.length) {
      lyricsBox.replaceChildren();
      noResults();
      return;
    }
    const playFrom = (i) => {
      ctx.app.keepSearch(query);
      ctx.playback.playList(
        `Lyrics: ${query.trim()}`,
        "search",
        hits.map((x) => x.song),
        i,
      );
    };
    lyricsBox.replaceChildren(
      h(
        "section.results-section",
        h("h3.section", "In lyrics"),
        hits.slice(0, lyricsHits).map((x, i) =>
          h(
            "div.row.hit.song",
            {
              tabindex: "0",
              onclick: () => playFrom(i),
              onkeydown: (e) => e.key === "Enter" && playFrom(i),
            },
            thumb(x.song),
            h("span.main", h("span.title", x.song.title), h("span.sub", `“${x.line}”`)),
          ),
        ),
      ),
    );
    empty.replaceChildren();
  };

  let found = [];
  const noResults = () => {
    const q = query.trim();
    empty.replaceChildren(
      q && !found.length && !lyricsBox.childElementCount
        ? emptyNote(
            `Nothing matches “${q}”`,
            "Words can be in any order; artist:, album:, year:1990-1999 narrow a word.",
            "search",
          )
        : "",
    );
  };

  const draw = () => {
    const q = new Query(query);
    expanded = query === draw.last ? expanded : new Set();
    draw.last = query;
    recent.replaceChildren(...(q.empty ? recentSearches() : []));
    found = q.empty ? [] : find(q);
    const songsAt = found.findIndex((f) => f.kind === "Songs");
    const songs = songsAt >= 0 ? found[songsAt].rows : [];
    before.replaceChildren(
      ...found.slice(0, songsAt < 0 ? found.length : songsAt).map(sectionEl),
    );
    after.replaceChildren(
      ...(songsAt < 0 ? [] : found.slice(songsAt + 1).map(sectionEl)),
    );
    songsHead.hidden = !songs.length;
    // "Search: …", as on the phone: a bare query could name, and refill, an album's queue.
    list.o.name = `Search: ${query.trim()}`;
    list.set(items(songs));
    lyricsSeq++;
    lyricsBox.replaceChildren();
    if (q.lyrics.length >= 3) {
      lyricsBox.replaceChildren(
        h("p.searching", h("span.spinner.small"), "Searching lyrics…"),
      );
      fetchLyrics(q, lyricsSeq);
    }
    noResults();
  };

  const recentSearches = () => {
    const r = ctx.app.recentSearches;
    if (!r.length)
      return [
        emptyNote(
          "Search your library",
          "Songs, albums, artists, folders, playlists and lyrics, forgiving spelling and script.",
          "search",
        ),
      ];
    return [
      h("h3.section", "Recent searches"),
      ...r.map((s) =>
        h(
          "button.row.recentrow",
          { type: "button", onclick: () => ctx.nav.go("search", s) },
          icon("recent"),
          h("span.main", h("span.title", s)),
        ),
      ),
      h(
        "button.link",
        { type: "button", onclick: () => (ctx.app.forgetSearches(), draw()) },
        "Clear recent searches",
      ),
    ];
  };

  draw();
  const el = frame({ title: "Search", body: list.el, cls: "searchpage" });
  if (input)
    el.querySelector(".pagebar").replaceChildren(
      h("label.searchfield", icon("search"), input),
    );
  return {
    el,
    title: "Search",
    topics: new Set(["catalog", "playlists", "thumbs", "playback"]),
    update(t) {
      if (t.has("catalog") || t.has("playlists")) draw();
      else list.refresh();
    },
    /** The address changed while this page shows: the field at the top of the window typed. */
    setQuery(q) {
      if (q === query) return;
      query = q;
      if (input) input.value = q;
      draw();
    },
    focus: () => input?.focus(),
  };
}
