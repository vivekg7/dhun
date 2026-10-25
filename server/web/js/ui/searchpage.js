// What search finds (plan 029), as on the phone and the Mac: artists, albums,
// genres, folders, playlists, songs and lyrics, the section with the best
// match first. A song plays the songs found; anything else opens over the
// results, so Back returns to them.

import { h, count, debounce } from "../util.js";
import { icon } from "../icons.js";
import { Query, nameIndex, lyricsHits } from "../search.js";
import { emptyNote, contextMenu } from "./kit.js";
import { SongList, thumb, items, songMenu } from "./songs.js";
import { playShelf } from "./spoken.js";
import { day } from "../catalog.js";
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
  const group = (kind) => (g) => ({
    title: g.name,
    detail: count(g.songs.length),
    song: g.songs[0],
    href: ctx.nav.href(kind, g.name),
  });
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
    section("Artists", c.index("artists"), group("artist")),
    section("Albums", c.index("albums"), (a) => ({
      title: a.name,
      detail: [a.artist, a.year > 0 ? a.year : ""].filter(Boolean).join(" · "),
      song: a.songs[0],
      href: ctx.nav.href("album", a.id),
    })),
    section("Genres", c.index("genres"), group("genre")),
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
    // Under their own headings (plan 031), before the songs, which can run to
    // hundreds and would bury them.
    section("Audiobooks", c.index("books"), (b) => ({
      title: b.title,
      detail: [b.author, app.heard.progress(b)].filter(Boolean).join(" · "),
      song: b.cover ?? b.items[0],
      href: ctx.nav.href("shelf", b.id),
    })),
    section("Podcasts", c.index("shows"), (x) => ({
      title: x.title,
      detail: "Podcast",
      song: x.cover ?? x.items[0],
      href: ctx.nav.href("shelf", x.id),
    })),
    // An episode found plays its show from there, as on the show's page.
    section("Episodes", c.index("episodes"), (s) => ({
      title: s.title,
      detail: [s.groupTitle, day(s)].filter(Boolean).join(" · "),
      song: s,
      play: () => {
        const shelf = c.shelfOf(s);
        if (shelf) playShelf(shelf, s);
      },
    })),
    section("Songs", c.index("songs"), (s) => s, boost),
  ];
  // Stable, so a tie keeps the order above.
  return found
    .map((f, i) => f && { ...f, i })
    .filter(Boolean)
    .sort((a, b) => a.best - b.best || a.i - b.i);
}

/** The search field: the window's, or on a phone the page's own. */
export const searchField = (value) =>
  h("input.searchinput", {
    type: "search",
    value,
    placeholder: "Songs, albums, artists, lyrics",
    "aria-label": "Search",
    autocomplete: "off",
    spellcheck: false,
  });

/** A result that plays rather than opens, by a click or Return. */
const playRow = (play, ...children) =>
  h(
    "div.row.hit.song",
    { tabindex: "0", onclick: play, onkeydown: (e) => e.key === "Enter" && play() },
    children,
  );

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
  const input = field ? searchField(initial) : null;
  input?.addEventListener("input", () => {
    query = input.value;
    ctx.nav.replace("search", query);
    draw();
  });
  const empty = h("div");
  list.el.append(empty);

  // Chips that jump to a section (plan 029): in the scroller itself, not its
  // head, so they stay in view under the field for the whole list.
  const chips = h("nav.jumps", { "aria-label": "Jump to a section", hidden: true });
  list.el.prepend(chips);
  let heads = [];
  // In the order they are on screen: the songs' rows come between.
  const headings = () => [
    ...before.querySelectorAll("h3.section"),
    ...(songsHead.hidden ? [] : [songsHead]),
    ...after.querySelectorAll("h3.section"),
    ...lyricsBox.querySelectorAll("h3.section"),
  ];
  const below = (hd) =>
    hd.getBoundingClientRect().top -
    list.el.getBoundingClientRect().top -
    chips.offsetHeight;
  const jump = (hd) =>
    list.el.scrollTo({ top: list.el.scrollTop + below(hd), behavior: "smooth" });
  /** The chip of the section at the top; at the bottom, the last, which may never reach the top. */
  const markChip = () => {
    if (!heads.length) return;
    const el = list.el;
    const end = el.scrollTop + el.clientHeight >= el.scrollHeight - 1;
    const at = Math.max(
      0,
      end ? heads.length - 1 : heads.findLastIndex((hd) => below(hd) <= 1),
    );
    const b = chips.children[at];
    if (b.hasAttribute("aria-current")) return;
    for (const c of chips.children) c.removeAttribute("aria-current");
    b.setAttribute("aria-current", "location");
    // On a phone the row runs past the edge: keep the selected chip in sight.
    // Only on a change, as scroll events come many a second.
    if (
      b.offsetLeft < chips.scrollLeft ||
      b.offsetLeft + b.offsetWidth > chips.scrollLeft + chips.clientWidth
    )
      chips.scrollTo({ left: b.offsetLeft - 20, behavior: "smooth" });
  };
  list.el.addEventListener("scroll", markChip, { passive: true });
  // With one section there is nowhere to jump.
  const drawChips = () => {
    const hs = headings();
    heads = hs.length > 1 ? hs : [];
    chips.hidden = !heads.length;
    chips.replaceChildren(
      ...heads.map((hd) =>
        h(
          "button",
          { type: "button", onclick: () => jump(hd) },
          hd.textContent === "In lyrics" ? "Lyrics" : hd.textContent,
        ),
      ),
    );
    markChip();
  };

  const sectionEl = (f) => {
    const whole = expanded.has(f.kind);
    const rows = whole ? f.rows : f.rows.slice(0, shown);
    return h(
      "section.results-section",
      h("h3.section", f.kind),
      rows.map((r) => {
        const inner = [
          r.icon ? h("span.thumb.folder-icon", icon(r.icon)) : thumb(r.song),
          h("span.main", h("span.title", r.title), h("span.sub", r.detail)),
        ];
        if (!r.play)
          return h(
            "a.row.hit",
            { href: r.href, onclick: () => ctx.app.keepSearch(query) },
            inner,
          );
        const play = () => {
          ctx.app.keepSearch(query);
          r.play();
        };
        return contextMenu(playRow(play, inner), () => songMenu([r.song]), r.title);
      }),
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
      showLyrics(songs);
    } catch {
      if (seq === lyricsSeq) {
        lyricsBox.replaceChildren();
        drawChips();
      }
    }
  }, 400);

  const showLyrics = (hits) => {
    if (!hits.length) {
      lyricsBox.replaceChildren();
      drawChips();
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
        hits
          .slice(0, lyricsHits)
          .map((x, i) =>
            playRow(
              () => playFrom(i),
              thumb(x.song),
              h(
                "span.main",
                h("span.title", x.song.title),
                h("span.sub", `“${x.line}”`),
              ),
            ),
          ),
      ),
    );
    empty.replaceChildren();
    drawChips();
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
    drawChips();
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
    topics: new Set(["catalog", "playlists", "thumbs", "playback", "resumes"]),
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
