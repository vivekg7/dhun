// The library as the pages show it, derived from the song list in memory, as
// on the phone and the Mac: about 7,000 songs group in a few milliseconds,
// simpler than keeping albums, artists and folders as tables of their own.
// Built whole, and never changed after.

import { natural, compareText, shuffled, nowMs, parseTime } from "./util.js";
import { songIndex, nameIndex, Index, Query } from "./search.js";

/** A song as the server sends it (api/openapi.yaml `Song`), with every optional field filled in. */
export function song(s) {
  const path = s.path ?? "";
  const slash = path.lastIndexOf("/");
  const artist = s.artist ?? "";
  const albumArtist = s.albumArtist ?? "";
  return {
    id: s.id,
    path,
    title: s.title ?? "",
    artist,
    artists: s.artists ?? [],
    album: s.album ?? "",
    albumArtist,
    composer: s.composer ?? "",
    genres: s.genres ?? [],
    year: s.year ?? 0,
    track: s.track ?? 0,
    disc: s.disc ?? 0,
    durationMs: s.durationMs ?? 0,
    format: s.format ?? "",
    codec: s.codec ?? "",
    bitrate: s.bitrate ?? 0,
    sampleRate: s.sampleRate ?? 0,
    bitDepth: s.bitDepth ?? 0,
    size: s.size ?? 0,
    hasLyrics: !!s.hasLyrics,
    addedAt: typeof s.addedAt === "number" ? s.addedAt : parseTime(s.addedAt),
    missing: !!s.missing,
    // Which cover it shows, shared by songs showing the same one (plan 019).
    art: s.art ?? "",
    folder: slash < 0 ? "" : path.slice(0, slash),
    displayArtist: artist || albumArtist || "Unknown artist",
    // Podcasts and audiobooks (plan 031); "" for music.
    kind: s.kind ?? "",
    spoken: s.kind === podcast || s.kind === audiobook,
    date: s.date ?? "",
    notes: s.notes ?? "",
    chapters: (Array.isArray(s.chapters) ? s.chapters : [])
      .filter((c) => typeof c?.startMs === "number")
      .map((c) => ({ startMs: c.startMs, title: c.title ?? "" })),
    group: s.group ?? "",
    groupTitle: s.groupTitle ?? "",
    groupIndex: s.groupIndex ?? 0,
  };
}

export const podcast = "podcast";
export const audiobook = "audiobook";

/** A chapter's own name, or "" when it is only a number ("001"), which says nothing "Chapter 1" does not. */
export const chapterName = (c) => (/^\s*\d*\s*$/.test(c.title) ? "" : c.title.trim());

/** The date tag as a day: some tools write "20251230", others a full time. */
export function day(s) {
  const d = s.date.trim();
  return /^\d{8}$/.test(d)
    ? `${d.slice(0, 4)}-${d.slice(4, 6)}-${d.slice(6)}`
    : d.slice(0, 10);
}

const trackOrder = (a, b) =>
  a.disc - b.disc || a.track - b.track || natural(a.title, b.title);

export class Catalog {
  constructor(all) {
    const present = all.filter((s) => !s.missing);
    // The music: what every song list, view and search of songs shows. Podcasts
    // and audiobooks have their own pages, so an episode never turns up in an
    // artist or a shuffle (plan 031).
    this.songs = present
      .filter((s) => !s.spoken)
      .sort((a, b) => natural(a.title, b.title));
    // Every song, episodes and books' files too: what queues, Favorites and Listen Later name.
    this.byId = new Map(present.map((s) => [s.id, s]));
    this.albums = albums(this.songs);
    this.albumById = new Map(this.albums.map((a) => [a.id, a]));
    this.albumOf = new Map();
    for (const a of this.albums) for (const s of a.songs) this.albumOf.set(s.id, a);
    this.artists = group(this.songs, (s) =>
      s.artists.length ? s.artists : [s.displayArtist],
    );
    this.genres = group(this.songs, (s) =>
      s.genres.length ? s.genres : ["Unknown genre"],
    );
    this.folders = folders(
      this.songs,
      present.filter((s) => s.spoken),
    );
    this.books = shelves(present, audiobook);
    this.shows = shelves(present, podcast);
    this.shelves = new Map([...this.books, ...this.shows].map((x) => [x.id, x]));
    this.indexes = {};
  }

  /** The book or show `s` is in, or undefined for music. */
  shelfOf(s) {
    return s?.spoken ? this.shelves.get(shelfKey(s)) : undefined;
  }

  /** The song whose cover `s` shows: an episode or book file without one shows its show's or book's. */
  artOf(s) {
    if (!s?.spoken || s.art) return s;
    return this.shelfOf(s)?.cover ?? s;
  }

  songsOf(ids) {
    const out = [];
    for (const id of ids) {
      const s = this.byId.get(id);
      if (s) out.push(s);
    }
    return out;
  }

  artist(name) {
    const l = name.toLowerCase();
    return this.artists.find((g) => g.id === l);
  }

  genre(name) {
    const l = name.toLowerCase();
    return this.genres.find((g) => g.id === l);
  }

  // Search indexes (plan 029), each built on first use: most catalogues are never searched.
  index(kind) {
    return (this.indexes[kind] ??= this.makeIndex(kind));
  }

  makeIndex(kind) {
    switch (kind) {
      case "songs":
        return songIndex(this.songs);
      case "albums":
        return new Index(this.albums, [
          ["album", 0, (a) => a.name],
          ["artist", 1, (a) => a.artist],
        ]);
      case "artists":
        return nameIndex(this.artists, "artist", (g) => g.name);
      case "genres":
        return nameIndex(this.genres, "genre", (g) => g.name);
      case "folders": {
        const all = [...this.folders.values()]
          .filter((f) => f.path !== "")
          .sort((a, b) => natural(a.path, b.path));
        return nameIndex(all, "folder", (f) => f.name);
      }
      case "books":
        return new Index(this.books, [
          ["book", 0, (b) => b.title],
          ["artist", 1, (b) => b.author],
        ]);
      case "shows":
        return nameIndex(this.shows, "show", (x) => x.title);
      case "episodes":
        return songIndex(this.shows.flatMap((x) => x.items));
    }
  }

  /** The songs of `list` that `query` finds, in the list's order: what a page's filter box shows. */
  filter(list, query) {
    const q = new Query(query);
    if (q.empty) return list;
    // Favorites, Listen Later and queues may hold episodes, which the music's index leaves out.
    const index = list.some((s) => s.spoken) ? songIndex(list) : this.index("songs");
    const found = new Set(index.filter(q));
    return list.filter((s) => found.has(s));
  }
}

/**
 * Two songs are on one album when they share the album name and either the
 * album artist or, without one, the folder. A bare album name would merge
 * every "Greatest Hits" in the collection.
 */
function albums(songs) {
  const groups = new Map();
  for (const s of songs) {
    if (!s.album) continue;
    const k =
      s.album.toLowerCase() +
      "\u0000" +
      (s.albumArtist ? s.albumArtist.toLowerCase() : s.folder);
    let g = groups.get(k);
    if (!g) groups.set(k, (g = []));
    g.push(s);
  }
  const out = [];
  for (const [id, list] of groups) {
    list.sort(trackOrder);
    const first = list[0];
    out.push({
      id,
      name: first.album,
      artist: first.albumArtist || mostCommon(list.map((s) => s.displayArtist)),
      year: Math.max(...list.map((s) => s.year)),
      songs: list,
    });
  }
  return out.sort((a, b) => natural(a.name, b.name));
}

function group(songs, keys) {
  const map = new Map();
  for (const s of songs)
    for (const k of keys(s)) {
      const l = k.toLowerCase();
      let g = map.get(l);
      if (!g) map.set(l, (g = { id: l, name: k, songs: [] }));
      g.songs.push(s);
    }
  return [...map.values()].sort((a, b) => natural(a.name, b.name));
}

const spokenRoot = (s) => (s.kind === podcast ? "Podcasts" : "Audiobooks");

/**
 * Every folder, keyed by its path relative to the Music folder ("" is the
 * top). Episodes and books' files sit under a Podcasts and an Audiobooks
 * root after the music's folders, as on the phone: their paths are relative
 * to folders of their own, so a show's folder could share a music folder's
 * name.
 */
function folders(songs, spoken) {
  const map = new Map();
  const folder = (path) => {
    let f = map.get(path);
    if (f) return f;
    const slash = path.lastIndexOf("/");
    f = {
      path,
      name: slash < 0 ? path || "Music" : path.slice(slash + 1),
      children: [],
      songs: [],
    };
    map.set(path, f);
    if (path !== "") folder(slash < 0 ? "" : path.slice(0, slash)).children.push(f);
    return f;
  };
  folder("");
  for (const s of songs) folder(s.folder).songs.push(s);
  for (const s of spoken)
    folder(`${spokenRoot(s)}/${s.folder}`.replace(/\/$/, "")).songs.push(s);
  const last = (f) => f.path === "Podcasts" || f.path === "Audiobooks";
  for (const f of map.values()) {
    f.children.sort((a, b) => last(a) - last(b) || natural(a.name, b.name));
    f.songs.sort(
      (a, b) => a.disc - b.disc || a.track - b.track || natural(a.path, b.path),
    );
  }
  return map;
}

/** A file the server did not group (none should be) is a shelf of its folder. */
const shelfKey = (s) => s.group || `${s.kind}\u0000${s.folder}`;

/**
 * Audiobooks or podcast shows as the server grouped them (plan 031): a
 * book's files in reading order, a show's episodes oldest first.
 */
function shelves(all, kind) {
  const groups = new Map();
  for (const s of all) {
    if (s.kind !== kind) continue;
    const k = shelfKey(s);
    let g = groups.get(k);
    if (!g) groups.set(k, (g = []));
    g.push(s);
  }
  const out = [];
  for (const [id, list] of groups) {
    list.sort((a, b) => a.groupIndex - b.groupIndex || natural(a.path, b.path));
    const first = list[0];
    out.push({
      id,
      kind,
      isBook: kind === audiobook,
      title: first.groupTitle || first.folder.slice(first.folder.lastIndexOf("/") + 1),
      // The author, or the host; "" when the tags name none.
      author: mostCommon(list.map((s) => s.artist || s.albumArtist).filter(Boolean)),
      items: list,
      // The first file with a cover: the one its files without one show.
      cover: list.find((s) => s.art),
    });
  }
  return out.sort((a, b) => natural(a.title, b.title));
}

/** A folder's songs and every subfolder's, in folder order: what "play folder" plays. */
export const allSongs = (f) => [...f.songs, ...f.children.flatMap(allSongs)];

function mostCommon(names) {
  const n = new Map();
  let best = "";
  for (const s of names) {
    n.set(s, (n.get(s) ?? 0) + 1);
    if (n.get(s) > (n.get(best) ?? 0)) best = s;
  }
  return best;
}

const byTitle = (a, b) => compareText(a.title, b.title);
const byAlbum = (a, b) =>
  compareText(a.album, b.album) || a.disc - b.disc || a.track - b.track;
// An artist's songs in album order, as on the phone.
const byArtist = (a, b) =>
  compareText(a.displayArtist, b.displayArtist) || byAlbum(a, b);
const sorted = (s, f) => s.slice().sort(f);

/** How a queue or playlist can be sorted: Musicolet's Randomize and Reverse, then by what a song's tags say. */
export const sorts = [
  ["Randomize", (s) => shuffled(s)],
  ["Reverse", (s) => s.slice().reverse()],
  ["Title, A to Z", (s) => sorted(s, byTitle)],
  ["Title, Z to A", (s) => sorted(s, byTitle).reverse()],
  ["Artist, A to Z", (s) => sorted(s, byArtist)],
  ["Artist, Z to A", (s) => sorted(s, byArtist).reverse()],
  ["Album, A to Z", (s) => sorted(s, byAlbum)],
  ["Album, Z to A", (s) => sorted(s, byAlbum).reverse()],
  ["Folder and file name", (s) => sorted(s, (a, b) => compareText(a.path, b.path))],
  ["Year, oldest first", (s) => sorted(s, (a, b) => a.year - b.year)],
  ["Year, newest first", (s) => sorted(s, (a, b) => b.year - a.year)],
  ["Shortest first", (s) => sorted(s, (a, b) => a.durationMs - b.durationMs)],
  ["Longest first", (s) => sorted(s, (a, b) => b.durationMs - a.durationMs)],
  ["Recently added first", (s) => sorted(s, (a, b) => b.addedAt - a.addedAt)],
];

/** An artist's or genre's songs by album, then disc and track: how they were released, as on the phone. */
export const albumOrder = (songs) => sorted(songs, byAlbum);

/** Each automatic view shows at most this many songs, as on the phone. */
const viewSize = 100;

/**
 * Favorites, Listen Later and the automatic views (plan 010). `source` is
 * what the server logs with each listen (plan 008); `mark` is the kind for
 * the two lists the user edits.
 */
export const lists = [
  {
    id: "favorites",
    title: "Favorites",
    icon: "heart",
    source: "favorites",
    mark: "fav",
  },
  {
    id: "listenLater",
    title: "Listen Later",
    icon: "later",
    source: "listen_later",
    mark: "later",
  },
  { id: "continue", title: "Continue listening", icon: "continue", source: "continue" },
  {
    id: "recentlyAdded",
    title: "Recently added",
    icon: "added",
    source: "recently_added",
  },
  {
    id: "recentlyPlayed",
    title: "Recently played",
    icon: "recent",
    source: "recently_played",
  },
  { id: "mostPlayed", title: "Most played", icon: "most", source: "most_played" },
  {
    id: "notLately",
    title: "Not played lately",
    icon: "lately",
    source: "not_played_lately",
  },
];

/** The songs of one of `lists`, from the app's state. */
export function listSongs(id, st) {
  const c = st.catalog;
  const stats = [...st.playStats.values()];
  const top = (ids) => c.songsOf(ids).slice(0, viewSize);
  switch (id) {
    case "favorites":
      return c.songsOf(st.marked("fav"));
    case "listenLater":
      return c.songsOf(st.marked("later"));
    case "continue":
      return c.songsOf(st.resumes());
    case "recentlyAdded":
      return c.songs
        .slice()
        .sort((a, b) => b.addedAt - a.addedAt)
        .slice(0, viewSize);
    case "recentlyPlayed":
      return top(
        stats
          .filter((p) => p.lastPlayedAt > 0)
          .sort((a, b) => b.lastPlayedAt - a.lastPlayedAt)
          .map((p) => p.song),
      );
    case "mostPlayed":
      return top(
        stats
          .filter((p) => p.count > 0)
          .sort((a, b) => b.count - a.count)
          .map((p) => p.song),
      );
    case "notLately": {
      // Played at least 3 times, and not in the last 90 days: favourites you have drifted away from.
      const cutoff = nowMs() - 90 * 86_400_000;
      return top(
        stats
          .filter((p) => p.count >= 3 && p.lastPlayedAt < cutoff)
          .sort((a, b) => b.count - a.count)
          .map((p) => p.song),
      );
    }
  }
  return [];
}

// MARK: What has been heard of the podcasts and audiobooks (plan 031)

/**
 * The spoken files marked played, and where the others were left: `played`
 * is a Set of song ids, `left` a Map of song id to the resume point in ms.
 */
export class Heard {
  constructor(played, left) {
    this.played = played;
    this.left = left;
  }

  heardMs(s) {
    if (this.played.has(s.id)) return s.durationMs;
    return Math.min(Math.max(this.left.get(s.id) ?? 0, 0), s.durationMs);
  }

  /** Where a book carries on: its first file not played, or from the start once all are. */
  next(shelf) {
    return shelf.items.find((s) => !this.played.has(s.id)) ?? shelf.items[0];
  }

  finished(shelf) {
    return shelf.items.every((s) => this.played.has(s.id));
  }

  /** Begun and not finished: its Play button says Continue. Finished, it starts again from the first. */
  begun(shelf) {
    return shelf.items.some((s) => this.heardMs(s) > 0) && !this.finished(shelf);
  }

  /** A book's progress, or its length before it is begun. */
  progress(shelf) {
    const total = shelf.items.reduce((a, s) => a + s.durationMs, 0);
    const heard = shelf.items.reduce((a, s) => a + this.heardMs(s), 0);
    if (this.finished(shelf)) return "Finished";
    if (heard <= 0 || total <= 0) return length(total);
    return `${Math.floor((heard * 100) / total)}% · ${length(total - heard)} left`;
  }

  /** A file's line beside its title: played, or how much is left of one begun. */
  state(s) {
    if (this.played.has(s.id)) return "Played";
    const h = this.heardMs(s);
    return h > 0 ? `${length(s.durationMs - h)} left` : "";
  }
}

/** "48 min", "12 h 5 min" */
export function length(ms) {
  const min = Math.floor(ms / 60_000);
  return min >= 60 ? `${Math.floor(min / 60)} h ${min % 60} min` : `${min} min`;
}
