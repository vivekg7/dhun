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
  };
}

const trackOrder = (a, b) =>
  a.disc - b.disc || a.track - b.track || natural(a.title, b.title);

export class Catalog {
  constructor(all) {
    this.songs = all
      .filter((s) => !s.missing)
      .sort((a, b) => natural(a.title, b.title));
    this.byId = new Map(this.songs.map((s) => [s.id, s]));
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
    this.folders = folders(this.songs);
    this.indexes = {};
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
    }
  }

  /** The songs of `list` that `query` finds, in the list's order: what a page's filter box shows. */
  filter(list, query) {
    const q = new Query(query);
    if (q.empty) return list;
    const found = new Set(this.index("songs").filter(q));
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

/** Every folder, keyed by its path relative to the Music folder ("" is the top). */
function folders(songs) {
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
  for (const f of map.values()) {
    f.children.sort((a, b) => natural(a.name, b.name));
    f.songs.sort(
      (a, b) => a.disc - b.disc || a.track - b.track || natural(a.path, b.path),
    );
  }
  return map;
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
