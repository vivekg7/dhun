import DhunKit
import SwiftUI

/// The sections of search results (plan 029), as on the phone.
private enum Kind: String, Hashable, Sendable {
    case artists = "Artists", albums = "Albums", genres = "Genres"
    case folders = "Folders", playlists = "Playlists"
    // Under their own headings (plan 031), before the songs, which can run to
    // hundreds and would bury them.
    case books = "Audiobooks", shows = "Podcasts", episodes = "Episodes"
    case songs = "Songs"
}

/// One result, ready to show. `route` is what it opens; a song or an episode has none, it plays.
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
private func find(
    _ c: Catalog, _ playlists: [Playlist], _ heard: Heard, _ q: SearchQuery, boost: (Song) -> Int
) -> [Found] {
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
        section(.books, c.bookIndex) {
            Row(
                id: "shelf/\($0.id)", title: $0.title,
                detail: [$0.author, heard.progress($0)].filter { !$0.isEmpty }.joined(separator: " · "),
                song: $0.cover ?? $0.items[0], route: .shelf($0.id))
        },
        section(.shows, c.showIndex) {
            Row(
                id: "shelf/\($0.id)", title: $0.title, detail: "Podcast", song: $0.cover ?? $0.items[0],
                route: .shelf($0.id))
        },
        // An episode found plays its show from there, as on the show's page.
        section(.episodes, c.episodeIndex) {
            Row(
                id: "e/\($0.id)", title: $0.title,
                detail: [$0.groupTitle, $0.day].filter { !$0.isEmpty }.joined(separator: " · "), song: $0)
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
    /// Each section heading's last top in the list, for the chip of the
    /// section at the top. A heading that scrolled away keeps its last
    /// place: above the top if it left there, below the bottom if not.
    @State private var tops: [String: CGFloat] = [:]
    /// The list's own top and height, in the same space as `tops`.
    @State private var listFrame = CGRect(x: 0, y: 0, width: 0, height: 600)
    /// The chip last tapped, selected until the jump settles and the list
    /// moves again: a Mac List reports no scroll phases to tell a hand's scroll by.
    @State private var tapped: String?
    @State private var tappedAt = Date.distantPast

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
        // The chips' names, in the sections' order (plan 029): Lyrics only once it has some.
        let chips = found.map(\.kind.rawValue) + (lyrics?.isEmpty == false ? ["Lyrics"] : [])
        let atTop = chips.last { (tops[$0] ?? .infinity) - listFrame.minY <= 8 } ?? chips.first
        ScrollViewReader { proxy in
            VStack(spacing: 0) {
                if chips.count > 1 {
                    ScrollView(.horizontal, showsIndicators: false) {
                        HStack(spacing: 6) {
                            ForEach(chips, id: \.self) { c in
                                let on = c == (tapped ?? atTop)
                                Button(c) {
                                    tapped = c
                                    tappedAt = .now
                                    // To the section's first row: a Mac List scrolls to rows, not to
                                    // headers. A header's height lower, so the pinned header does not cover it.
                                    let below = UnitPoint(x: 0, y: 30 / max(listFrame.height, 30))
                                    let to: AnyHashable? =
                                        c == "Lyrics"
                                        ? lyrics?.first.map { AnyHashable($0.song.id) }
                                        : found.first { $0.kind.rawValue == c }?.rows.first.map {
                                            AnyHashable($0.id)
                                        }
                                    guard let to else { return }
                                    // Rows far off are placed by estimate until laid out, so a long
                                    // jump lands short: again, once the rows around it exist.
                                    withAnimation { scroll(proxy, to, below) }
                                    Task {
                                        for _ in 0..<3 {
                                            try? await Task.sleep(for: .milliseconds(150))
                                            scroll(proxy, to, below)
                                        }
                                    }
                                }
                                .buttonStyle(.plain)
                                .padding(.horizontal, 10).padding(.vertical, 4)
                                .foregroundStyle(on ? AnyShapeStyle(.white) : AnyShapeStyle(.primary))
                                .background(
                                    Capsule().fill(on ? AnyShapeStyle(.tint) : AnyShapeStyle(.quaternary)))
                            }
                        }
                        .padding(.horizontal, 12).padding(.vertical, 6)
                    }
                }
                results(app, nav, found, songs, lyrics)
                    .onGeometryChange(for: CGRect.self) {
                        $0.frame(in: .global)
                    } action: {
                        listFrame = $0
                    }
            }
        }
        .onChange(of: query) {
            expanded = []
            tops = [:]
            tapped = nil
        }
    }

    /// `scrollTo` matches an id by its own type, so an `AnyHashable` is unwrapped first.
    private func scroll(_ proxy: ScrollViewProxy, _ id: AnyHashable, _ anchor: UnitPoint) {
        if let s = id.base as? String {
            proxy.scrollTo(s, anchor: anchor)
        } else if let n = id.base as? Int {
            proxy.scrollTo(n, anchor: anchor)
        }
    }

    /// A section's heading, whose place says which chip is on.
    private func heading(_ title: String, chip: String? = nil) -> some View {
        let id = chip ?? title
        return Text(title)
            .onGeometryChange(for: CGFloat.self) {
                $0.frame(in: .global).minY
            } action: {
                tops[id] = $0
                // Moved after the jump settled (it scrolls for under half a second): the hand's.
                if tapped != nil, Date.now.timeIntervalSince(tappedAt) > 0.8 { tapped = nil }
            }
    }

    private func results(
        _ app: AppModel, _ nav: Nav, _ found: [Found], _ songs: [Song],
        _ lyrics: [(song: Song, line: String)]?
    )
        -> some View
    {
        List(selection: $selection) {
            ForEach(found, id: \.kind) { f in
                SwiftUI.Section {
                    if f.kind == .songs {
                        ForEach(f.rows) { r in
                            SongRow(app: app, song: r.song!, detail: r.detail, size: 30).tag(r.id)
                        }
                    } else {
                        let whole = expanded.contains(f.kind)
                        ForEach(whole ? f.rows : Array(f.rows.prefix(shown))) { r in
                            if let route = r.route {
                                Button {
                                    app.keepSearch(query)
                                    nav.path.append(route)
                                } label: {
                                    HitRow(app: app, row: r)
                                }
                                .buttonStyle(.plain)
                            } else {
                                SongRow(app: app, song: r.song!, detail: r.detail, size: 30).tag(r.id)
                            }
                        }
                        if !whole && f.rows.count > shown {
                            Button("Show all \(f.rows.count) \(f.kind.rawValue.lowercased())") {
                                expanded.insert(f.kind)
                            }
                            .buttonStyle(.link)
                        }
                    }
                } header: {
                    heading(f.kind.rawValue)
                }
            }
            if lyrics?.isEmpty != true {
                SwiftUI.Section {
                    if let lyrics {
                        ForEach(lyrics, id: \.song.id) { s, line in
                            SongRow(app: app, song: s, detail: "“\(line)”", size: 30).tag("l/\(s.id)")
                        }
                    } else {
                        HStack {
                            ProgressView().controlSize(.small)
                            Text("Searching lyrics…").foregroundStyle(.secondary)
                        }
                    }
                } header: {
                    heading("In lyrics", chip: "Lyrics")
                }
            }
        }
        .contextMenu(forSelectionType: String.self) { tags in
            let (name, list, chosen) = picked(tags, songs, lyrics?.map(\.song) ?? [])
            SongMenu(songs: chosen, queueName: name, source: "search", all: list)
                .environment(app)
                .environment(nav)
        } primaryAction: { tags in
            if let tag = tags.first(where: { $0.hasPrefix("e/") }), let id = Int(tag.dropFirst(2)),
                let s = app.catalog.byId[id], let shelf = app.catalog.shelfOf(s)
            {
                app.keepSearch(query)
                return playShelf(app, shelf, from: s)
            }
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
        // Off the main thread: the first search builds the catalog's indexes.
        .task(id: Key(query: query, catalog: ObjectIdentifier(app.catalog), playlists: app.playlists)) {
            let c = app.catalog
            let playlists = app.playlists
            let q = SearchQuery(query)
            let favs = Set(app.favorites.map(\.song))
            let plays = app.playStats.mapValues(\.count)
            let heard = app.heard
            let result = await Task.detached {
                find(c, playlists, heard, q) { s in
                    (favs.contains(s.id) ? 3 : 0) + min(2, (plays[s.id] ?? 0) / 5)
                }
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
        // Episodes play their show, not a list of what was found; the menu has no Play for them.
        if tags.contains(where: { $0.hasPrefix("e/") }) {
            return (
                "", [],
                tags.compactMap { $0.hasPrefix("e/") ? Int($0.dropFirst(2)) : nil }.compactMap {
                    app.catalog.byId[$0]
                }
            )
        }
        // "Search: …", as on the phone: a bare query could name, and refill, an album's queue.
        if tags.contains(where: { $0.hasPrefix("s/") }) {
            return ("Search: \(q)", songs, songs.filter { tags.contains("s/\($0.id)") })
        }
        return ("Lyrics: \(q)", inLyrics, inLyrics.filter { tags.contains("l/\($0.id)") })
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
