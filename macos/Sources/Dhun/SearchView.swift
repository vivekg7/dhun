import DhunKit
import SwiftUI

/// The sections of search results (plan 029), as on the phone.
private enum Kind: String, Hashable, Sendable {
    case artists = "Artists", albums = "Albums", genres = "Genres"
    case folders = "Folders", playlists = "Playlists", songs = "Songs"
}

/// One result, ready to show. `route` is what it opens; a song has none, it plays.
private struct Row: Identifiable, Sendable {
    let id: String
    let title: String
    let detail: String
    /// Whose cover it shows, or the song itself.
    let song: Song?
    var icon: String?
    var route: Route?
}

private struct Found: Sendable {
    let kind: Kind
    let rows: [Row]
    let best: Int
}

/// Results a section shows before "Show all": one strong song should not sit under twenty albums.
private let shown = 3

/// Songs found are many in a big library; past this many, the query is too loose to read through.
private let songsShown = 300

/// Every section with something in it, the one with the best match first; on a tie, in this order.
private func find(_ c: Catalog, _ playlists: [Playlist], _ q: SearchQuery, boost: (Song) -> Int) -> [Found] {
    if q.isEmpty { return [] }
    func section<T>(_ kind: Kind, _ index: SearchIndex<T>, boost: (T) -> Int = { _ in 0 }, _ row: (T) -> Row)
        -> Found?
    {
        let hits = index.search(q, boost: boost)
        guard let first = hits.first else { return nil }
        return Found(
            kind: kind, rows: hits.prefix(kind == .songs ? songsShown : hits.count).map { row($0.item) },
            best: first.score)
    }
    let found = [
        section(.artists, c.artistIndex) {
            Row(
                id: "artist/\($0.id)", title: $0.name, detail: count($0.songs.count), song: $0.songs.first,
                route: .artist($0.name))
        },
        section(.albums, c.albumIndex) {
            Row(
                id: "album/\($0.id)", title: $0.name,
                detail: [$0.artist, $0.year > 0 ? String($0.year) : ""].filter { !$0.isEmpty }.joined(
                    separator: " · "),
                song: $0.songs.first, route: .album($0.id))
        },
        section(.genres, c.genreIndex) {
            Row(
                id: "genre/\($0.id)", title: $0.name, detail: count($0.songs.count), song: $0.songs.first,
                route: .genre($0.name))
        },
        // Many folders share a name ("CD1"), so the path says which.
        section(.folders, c.folderIndex) { f in
            let parent = f.path.lastIndex(of: "/").map { String(f.path[..<$0]) } ?? "Music"
            return Row(
                id: "folder/\(f.path)", title: f.name, detail: parent.replacing("/", with: " › "),
                song: nil, icon: "folder", route: .folder(f.path))
        },
        section(.playlists, SearchIndex.names(playlists, "playlist") { $0.name }) {
            Row(
                id: "playlist/\($0.id)", title: $0.name, detail: count(c.songsOf($0.songs).count), song: nil,
                icon: $0.shared ? "music.note.house" : "music.note.list", route: .playlist($0.id))
        },
        section(.songs, c.songIndex, boost: boost) {
            Row(
                id: "s/\($0.id)", title: $0.title,
                detail: [$0.displayArtist, $0.album].filter { !$0.isEmpty }.joined(separator: " · "), song: $0
            )
        },
    ]
    // Stable, so a tie keeps the order above.
    return found.compactMap { $0 }.enumerated().sorted {
        ($0.element.best, $0.offset) < ($1.element.best, $1.offset)
    }
    .map(\.element)
}

/// What the window's search field finds: artists, albums, genres, folders,
/// playlists, songs and lyrics. Double-click or Return plays a song, from the
/// songs found; a click opens anything else over the results, so Back
/// returns to them.
struct SearchResults: View {
    @Environment(AppModel.self) private var app
    @Environment(Nav.self) private var nav
    let query: String
    @State private var found: [Found]?
    /// The server's, once typing stops; nil while it is asked.
    @State private var inLyrics: [(song: Song, line: String)]? = []
    @State private var expanded = Set<Kind>()
    @State private var selection = Set<String>()

    private struct Key: Hashable {
        let query: String
        let catalog: ObjectIdentifier
        let playlists: [Playlist]
    }

    var body: some View {
        // Values, not the environment, for the rows and menus built later (see `Thumb`).
        let app = self.app
        let nav = self.nav
        let found = self.found ?? []
        let songs = found.first { $0.kind == .songs }?.rows.compactMap(\.song) ?? []
        let lyrics = inLyrics
        List(selection: $selection) {
            ForEach(found, id: \.kind) { f in
                SwiftUI.Section(f.kind.rawValue) {
                    if f.kind == .songs {
                        ForEach(f.rows) { r in SongHit(app: app, song: r.song!, detail: r.detail).tag(r.id) }
                    } else {
                        let whole = expanded.contains(f.kind)
                        ForEach(whole ? f.rows : Array(f.rows.prefix(shown))) { r in
                            Button {
                                app.keepSearch(query)
                                nav.path.append(r.route!)
                            } label: {
                                HitRow(app: app, row: r)
                            }
                            .buttonStyle(.plain)
                        }
                        if !whole && f.rows.count > shown {
                            Button("Show all \(f.rows.count) \(f.kind.rawValue.lowercased())") {
                                expanded.insert(f.kind)
                            }
                            .buttonStyle(.link)
                        }
                    }
                }
            }
            if lyrics?.isEmpty != true {
                SwiftUI.Section("In lyrics") {
                    if let lyrics {
                        ForEach(lyrics, id: \.song.id) { s, line in
                            SongHit(app: app, song: s, detail: "“\(line)”").tag("l/\(s.id)")
                        }
                    } else {
                        HStack {
                            ProgressView().controlSize(.small)
                            Text("Searching lyrics…").foregroundStyle(.secondary)
                        }
                    }
                }
            }
        }
        .contextMenu(forSelectionType: String.self) { tags in
            let (name, list, chosen) = picked(tags, songs, lyrics?.map(\.song) ?? [])
            SongMenu(songs: chosen, queueName: name, source: "search", all: list)
                .environment(app)
                .environment(nav)
        } primaryAction: { tags in
            let (name, list, chosen) = picked(tags, songs, lyrics?.map(\.song) ?? [])
            guard let first = chosen.first, let i = list.firstIndex(of: first) else { return }
            app.keepSearch(query)
            app.playback.play(name: name, source: "search", songs: list, start: i)
        }
        .overlay {
            if self.found?.isEmpty == true && lyrics?.isEmpty == true {
                ContentUnavailableView.search(text: query)
            }
        }
        .navigationTitle("Search")
        .onChange(of: query) { expanded = [] }
        // Off the main thread: the first search builds the catalog's indexes.
        .task(id: Key(query: query, catalog: ObjectIdentifier(app.catalog), playlists: app.playlists)) {
            let c = app.catalog
            let playlists = app.playlists
            let q = SearchQuery(query)
            let favs = Set(app.favorites.map(\.song))
            let plays = app.playStats.mapValues(\.count)
            let result = await Task.detached {
                find(c, playlists, q) { s in (favs.contains(s.id) ? 3 : 0) + min(2, (plays[s.id] ?? 0) / 5) }
            }.value
            if !Task.isCancelled { self.found = result }
        }
        .task(id: Key(query: query, catalog: ObjectIdentifier(app.catalog), playlists: [])) {
            let q = SearchQuery(query)
            guard q.lyrics.count >= 3 else { return inLyrics = [] }
            inLyrics = nil
            try? await Task.sleep(for: .milliseconds(400))
            if Task.isCancelled { return }
            let hits = await app.lyrics.search(q)
            if Task.isCancelled { return }
            inLyrics = hits.compactMap { h in app.catalog.byId[h.song].map { ($0, h.line) } }
        }
    }

    /// The list a selection plays from, its queue's name, and the songs chosen: songs found, or lyrics found.
    private func picked(_ tags: Set<String>, _ songs: [Song], _ inLyrics: [Song]) -> (String, [Song], [Song])
    {
        let q = query.trimmingCharacters(in: .whitespaces)
        // "Search: …", as on the phone: a bare query could name, and refill, an album's queue.
        if tags.contains(where: { $0.hasPrefix("s/") }) {
            return ("Search: \(q)", songs, songs.filter { tags.contains("s/\($0.id)") })
        }
        return ("Lyrics: \(q)", inLyrics, inLyrics.filter { tags.contains("l/\($0.id)") })
    }
}

/// A song found: its cover, title, and the artist and album, or the line of lyrics.
private struct SongHit: View {
    let app: AppModel
    let song: Song
    let detail: String

    var body: some View {
        HStack(spacing: 8) {
            Thumb(app: app, song: song, size: 30)
            VStack(alignment: .leading, spacing: 1) {
                Text(song.title).lineLimit(1)
                Text(detail).font(.caption).foregroundStyle(.secondary).lineLimit(1)
            }
            Spacer()
            DownloadedMark(app: app, song: song)
            Text(song.durationMs > 0 ? clock(Double(song.durationMs) / 1000) : "").font(.caption)
                .monospacedDigit()
                .foregroundStyle(.secondary)
        }
        .opacity(playable(app, song) ? 1 : 0.4)
    }
}

/// An artist, album, genre, folder or playlist found.
private struct HitRow: View {
    let app: AppModel
    let row: Row

    var body: some View {
        HStack(spacing: 10) {
            if let icon = row.icon {
                Image(systemName: icon).font(.title3).foregroundStyle(.tint).frame(width: 30, height: 30)
            } else if let song = row.song {
                Thumb(app: app, song: song, size: 30)
            }
            VStack(alignment: .leading, spacing: 1) {
                Text(row.title).lineLimit(1)
                Text(row.detail).font(.caption).foregroundStyle(.secondary).lineLimit(1)
            }
            Spacer()
        }
        .contentShape(Rectangle())
    }
}

/// The last searches, offered under the empty search field; the phone lists them on its Search tab.
struct RecentSearches: View {
    @Environment(AppModel.self) private var app
    @Environment(Nav.self) private var nav

    var body: some View {
        if nav.search.isEmpty && !app.recentSearches.isEmpty {
            ForEach(app.recentSearches, id: \.self) { s in
                Label(s, systemImage: "clock.arrow.circlepath").searchCompletion(s)
            }
            Button("Clear Recent Searches") { app.forgetSearches() }
        }
    }
}
