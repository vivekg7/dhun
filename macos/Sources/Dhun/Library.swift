import DhunKit
import SwiftUI

/// A list row's cover: the thumbnail every cover has, never waiting (plan 019).
///
/// Rows take the app model as a value, not from the environment: when a
/// table or list is reused for another list's rows, macOS 26 can rebuild a
/// row before its environment is in place, and an `@Environment` read of the
/// model then traps.
struct Thumb: View {
    let app: AppModel
    let song: Song
    let size: CGFloat

    var body: some View {
        _ = app.thumbsArrived
        let song = app.catalog.artOf(self.song)
        return Group {
            if let img = app.covers.thumbnail(song) {
                Image(nsImage: img).resizable().aspectRatio(contentMode: .fill)
            } else {
                Placeholder()
            }
        }
        .frame(width: size, height: size)
        .clipShape(RoundedRectangle(cornerRadius: size / 8))
    }
}

/// A larger cover, fetched when first shown; the thumbnail until it
/// arrives, then the cover fades in over it. A cover already in memory is
/// shown at once, so a grid scrolled back does not flash.
struct Cover: View {
    @Environment(AppModel.self) private var app
    let song: Song?
    let size: CGFloat
    /// The cover fetched, and for which key: a view given another song must not show the last one's.
    @State private var loaded: (key: String, image: NSImage)?

    var body: some View {
        let song = self.song.map(app.catalog.artOf)
        let key = "\(song?.art ?? "")-\(song?.id ?? 0)-\(Int(size))"
        let px = Int(size * 2)
        let image = loaded?.key == key ? loaded?.image : song.flatMap { app.covers.cached($0, px: px) }
        Group {
            if let image {
                Image(nsImage: image).resizable().aspectRatio(contentMode: .fill)
            } else if let song, let t = app.covers.thumbnail(song) {
                Image(nsImage: t).resizable().aspectRatio(contentMode: .fill)
            } else {
                Placeholder()
            }
        }
        .frame(width: size, height: size)
        .clipShape(RoundedRectangle(cornerRadius: max(4, size / 24)))
        .task(id: key) {
            guard let song, image == nil, let img = await app.covers.image(song, px: px) else { return }
            withAnimation(.easeOut(duration: 0.2)) { loaded = (key, img) }
        }
    }
}

struct Placeholder: View {
    var body: some View {
        ZStack {
            Rectangle().fill(.tint.opacity(0.15))
            Image(systemName: "music.note").foregroundStyle(.tint.opacity(0.6))
        }
    }
}

struct AlbumsGrid: View {
    @Environment(AppModel.self) private var app
    @State private var filter = ""

    var body: some View {
        let albums = app.catalog.albums
        let shown = filter.isEmpty ? albums : app.catalog.albumIndex.filter(SearchQuery(filter))
        ScrollView {
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 150, maximum: 190), spacing: 18)], spacing: 22) {
                ForEach(shown) { a in
                    NavigationLink(value: Route.album(a.id)) {
                        VStack(alignment: .leading, spacing: 5) {
                            Cover(song: a.songs.first, size: 150)
                            HStack(spacing: 4) {
                                Text(a.name).font(.callout.weight(.medium)).lineLimit(1)
                                KeptMark(on: app.downloads.pinned(Downloads.album, a.id))
                            }
                            Text(a.artist).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                            Text(dotted([a.year > 0 ? String(a.year) : "", summary(a.songs)]))
                                .font(.caption).foregroundStyle(.secondary).lineLimit(1)
                        }
                        .frame(width: 150)
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .contextMenu {
                        CollectionMenu(
                            name: a.name, source: "album:\(a.id)", songs: a.songs,
                            pin: (Downloads.album, a.id))
                    }
                }
            }
            .padding(20)
        }
        .emptyOverlay(albums, shown, filter)
        .navigationTitle("Albums")
        .toolbar { FilterField(text: $filter, prompt: "Filter albums") }
    }
}

/// Shown while the library is still arriving, or empty.
struct Empty: View {
    @Environment(AppModel.self) private var app
    var body: some View {
        if app.catalog.songs.isEmpty && app.syncing {
            ProgressView("Fetching the library…")
        } else {
            ContentUnavailableView("Nothing here", systemImage: "music.note")
        }
    }
}

extension View {
    /// Over a list a filter narrows: `Empty` while the list is, else nothing found when nothing is `shown`.
    func emptyOverlay(_ all: some Collection, _ shown: some Collection, _ filter: String) -> some View {
        overlay {
            if all.isEmpty {
                Empty()
            } else if shown.isEmpty {
                ContentUnavailableView.search(text: filter)
            }
        }
    }
}

/// The menu of an album, artist, genre or folder in a list: play it, the song menu for all of it, download it.
struct CollectionMenu: View {
    @Environment(AppModel.self) private var app
    let name: String
    let source: String
    let songs: [Song]
    let pin: (kind: String, ref: String)

    var body: some View {
        Button("Play") { app.playback.play(name: name, source: source, songs: songs, start: 0) }
        SongMenu(songs: songs)
        PinButton(kind: pin.kind, ref: pin.ref, name: name)
    }
}

/// The phone has a search box on every list; here it filters the list in
/// place, matching as search does (plan 029). In the toolbar, not the page:
/// the window's first text field takes the focus, and Space must play and
/// pause, not type.
struct FilterField: ToolbarContent {
    @Binding var text: String
    let prompt: String
    var body: some ToolbarContent {
        ToolbarItem(placement: .principal) {
            TextField(prompt, text: $text).textFieldStyle(.roundedBorder).frame(width: 200)
        }
    }
}

/// Pages get a filter only past this many rows: a shorter list fits on about one screen.
let filterOver = 12

extension View {
    /// The filter of a page opened from a list (an album, a folder, a playlist), once it is long (plan 011).
    func filterBox(_ text: Binding<String>, rows: Int, prompt: String = "Filter songs") -> some View {
        toolbar { if rows > filterOver { FilterField(text: text, prompt: prompt) } }
    }
}

struct ArtistPage: View {
    @Environment(AppModel.self) private var app
    let group: DhunKit.Group
    @State private var query = ""

    var body: some View {
        let ids = Set(group.songs.compactMap { app.catalog.albumOf[$0.id] })
        let albums = app.catalog.albums.filter { ids.contains($0.id) }
        let songs = albumOrder(group.songs)
        VStack(spacing: 0) {
            ListHeader(
                title: group.name, subtitle: "\(albums.count) albums · " + summary(songs),
                songs: songs,
                source: "artist:\(group.name)", song: songs.first, pin: (Downloads.artist, group.name))
            if albums.count > 1 {
                ScrollView(.horizontal) {
                    LazyHStack(spacing: 14) {
                        ForEach(albums) { a in
                            NavigationLink(value: Route.album(a.id)) {
                                VStack(alignment: .leading, spacing: 3) {
                                    Cover(song: a.songs.first, size: 96)
                                    Text(a.name).font(.caption).lineLimit(1)
                                }
                                .frame(width: 96)
                            }
                            .buttonStyle(.plain)
                        }
                    }
                    .padding(.horizontal, 16)
                }
                .frame(height: 130)
            }
            SongTable(songs: songs, name: group.name, source: "artist:\(group.name)", query: query)
        }
        .navigationTitle(group.name)
        .filterBox($query, rows: songs.count)
    }
}

/// An artist's or genre's songs by album, then disc and track: how they were released, as on the phone.
func albumOrder(_ songs: [Song]) -> [Song] { QueueSort.album.apply(songs) }

/// "12 songs · 48:10"
func summary(_ songs: [Song]) -> String {
    summary(songs.count, songs.reduce(0) { $0 + $1.durationMs })
}

func summary(_ songs: Int, _ durationMs: Int) -> String {
    "\(count(songs)) · \(clock(Double(durationMs) / 1000))"
}

/// "1 song", "12 songs", "29 books"
func count(_ n: Int, _ what: String = "song") -> String { "\(n) \(what)" + (n == 1 ? "" : "s") }

/// The podcasts' and audiobooks' roots take their sidebar icons.
private func folderIcon(_ path: String) -> String {
    switch path {
    case Catalog.podcasts: "mic"
    case Catalog.audiobooks: "book"
    default: "folder"
    }
}

/// "3 folders · 120 songs · 7:45:00": what a folder holds, its subfolders' songs too.
func summary(_ f: Folder) -> String {
    let n = f.children.count
    return (n == 0 ? "" : n == 1 ? "1 folder · " : "\(n) folders · ") + summary(f.songCount, f.durationMs)
}

/// Artists or genres: a filtered list, each opening its songs.
struct GroupsList: View {
    enum Kind { case artist, genre }
    @Environment(AppModel.self) private var app
    let kind: Kind
    @State private var filter = ""

    var body: some View {
        let app = self.app
        let all = kind == .artist ? app.catalog.artists : app.catalog.genres
        let shown =
            filter.isEmpty
            ? all
            : (kind == .artist ? app.catalog.artistIndex : app.catalog.genreIndex).filter(SearchQuery(filter))
        let pinKind = kind == .artist ? Downloads.artist : Downloads.genre
        List(shown) { g in
            NavigationLink(value: kind == .artist ? Route.artist(g.name) : Route.genre(g.name)) {
                HStack(spacing: 10) {
                    Thumb(app: app, song: g.songs[0], size: 32)
                    VStack(alignment: .leading) {
                        HStack(spacing: 4) {
                            Text(g.name)
                            KeptMark(on: app.downloads.pinned(pinKind, g.name))
                        }
                        Text(summary(g.songs)).font(.caption).foregroundStyle(.secondary)
                    }
                }
            }
            .contextMenu {
                CollectionMenu(
                    name: g.name, source: "\(kind == .artist ? "artist" : "genre"):\(g.name)", songs: g.songs,
                    pin: (pinKind, g.name))
            }
        }
        .emptyOverlay(all, shown, filter)
        .navigationTitle(kind == .artist ? "Artists" : "Genres")
        .toolbar { FilterField(text: $filter, prompt: kind == .artist ? "Filter artists" : "Filter genres") }
    }
}

/// The real NAS folder tree: subfolders, then the folder's own songs.
struct FolderView: View {
    @Environment(AppModel.self) private var app
    let path: String
    @State private var query = ""

    var body: some View {
        if let f = app.catalog.folders[path] {
            let app = self.app
            let name = f.path.isEmpty ? "Folders" : f.name
            VStack(spacing: 0) {
                // A folder's header, even one holding only folders: it plays and downloads them all.
                if !f.path.isEmpty {
                    ListHeader(
                        title: name, subtitle: summary(f), songs: f.allSongs(),
                        source: "folder:\(f.path)",
                        pin: (Downloads.folder, f.path))
                }
                let children =
                    query.isEmpty
                    ? f.children
                    : SearchIndex.names(f.children, "folder") { $0.name }.filter(SearchQuery(query))
                if !children.isEmpty {
                    List(children) { c in
                        NavigationLink(value: Route.folder(c.path)) {
                            HStack(spacing: 10) {
                                Image(systemName: folderIcon(c.path)).foregroundStyle(.tint).frame(width: 22)
                                VStack(alignment: .leading) {
                                    HStack(spacing: 4) {
                                        Text(c.name)
                                        KeptMark(on: app.downloads.pinned(Downloads.folder, c.path))
                                    }
                                    Text(summary(c)).font(.caption).foregroundStyle(.secondary)
                                }
                            }
                        }
                        .contextMenu {
                            CollectionMenu(
                                name: c.name, source: "folder:\(c.path)", songs: c.allSongs(),
                                pin: (Downloads.folder, c.path))
                        }
                    }
                    .frame(minHeight: 120, idealHeight: f.songs.isEmpty ? .infinity : 220)
                }
                if !f.songs.isEmpty {
                    if !children.isEmpty { Divider() }
                    SongTable(songs: f.songs, name: name, source: "folder:\(f.path)", query: query)
                } else if children.isEmpty && !query.isEmpty {
                    ContentUnavailableView.search(text: query)
                }
            }
            .navigationTitle(name)
            .filterBox($query, rows: f.children.count + f.songs.count, prompt: "Filter folder")
        } else {
            Empty()
        }
    }
}

/// Favorites, Listen Later and the automatic views (plan 010).
struct ListPage: View {
    @Environment(AppModel.self) private var app
    let kind: ListKind
    @State private var query = ""

    var body: some View {
        let songs = kind.songs(app)
        VStack(spacing: 0) {
            ListHeader(
                title: kind.title, subtitle: summary(songs), songs: songs, source: kind.source,
                pin: kind.mark.map { (Downloads.list, $0) })
            SongTable(
                songs: songs, name: kind.title, source: kind.source, query: query,
                extraMenu: kind.mark.map { mark in
                    { ids in
                        AnyView(
                            Button("Remove from \(kind.title)") {
                                for id in ids { app.store.mark(mark, id, false) }
                            })
                    }
                })
        }
        .navigationTitle(kind.title)
        .filterBox($query, rows: songs.count)
    }
}

struct DownloadsView: View {
    @Environment(AppModel.self) private var app
    @State private var query = ""

    var body: some View {
        let st = app.downloads.status
        let all = app.catalog.songsOf(Array(app.downloaded.keys))
        // Apart from the songs, so playing every download plays music only (plan 031).
        let songs = all.filter { !$0.spoken }.sorted {
            $0.title.localizedCaseInsensitiveCompare($1.title) == .orderedAscending
        }
        let spoken = (app.catalog.books + app.catalog.shows).flatMap(\.items).filter {
            app.downloaded[$0.id] != nil
        }
        VStack(alignment: .leading, spacing: 0) {
            VStack(alignment: .leading, spacing: 6) {
                Text("Downloads").font(.title2.weight(.semibold))
                Text("\(st.done) of \(st.wanted) songs on this Mac · \(formatBytes(st.usedBytes)) used")
                    .foregroundStyle(.secondary)
                switch st.state {
                case .downloading:
                    if let s = st.current {
                        ProgressView(value: st.progress) { Text("Downloading “\(s.title)”").font(.caption) }
                    }
                case .noNetwork: Text("Waiting for the server").foregroundStyle(.orange)
                case .waitingForNetwork:
                    Text("Waiting for a network that is not a phone's hotspot").foregroundStyle(.orange)
                case .full:
                    Text("At the storage limit: \(formatBytes(st.needBytes)) more needed").foregroundStyle(
                        .orange)
                case .noSpace:
                    Text("The disk is full: \(formatBytes(st.needBytes)) more needed").foregroundStyle(
                        .orange)
                case .failed: Text("Failed: \(st.error). Trying again shortly.").foregroundStyle(.red)
                case .idle: EmptyView()
                }
            }
            .padding(16)
            // Kept albums, playlists and folders by name; songs as search finds them.
            let pins =
                query.isEmpty
                ? app.pins : SearchIndex.names(app.pins, "name") { $0.name }.filter(SearchQuery(query))
            List(pins) { p in
                HStack {
                    Image(systemName: icon(p)).foregroundStyle(.tint).frame(width: 22)
                    VStack(alignment: .leading) {
                        Text(p.name)
                        Text(kindLabel(p)).font(.caption).foregroundStyle(.secondary)
                    }
                    Spacer()
                    Button("Remove Download", systemImage: "xmark.circle") {
                        if confirmUnpin(p.name) { app.downloads.unpin(p.key) }
                    }
                    .labelStyle(.iconOnly).buttonStyle(.borderless).help("Remove the download")
                }
            }
            .frame(maxHeight: songs.isEmpty && spoken.isEmpty ? .infinity : 220)
            .overlay {
                if app.pins.isEmpty {
                    ContentUnavailableView(
                        "Nothing downloaded", systemImage: "arrow.down.circle",
                        description: Text(
                            "Download an album, a playlist or Favorites to play them without the server."))
                }
            }
            // The songs themselves, to play without the server, as on the phone.
            if !songs.isEmpty {
                Divider()
                ListHeader(
                    title: "Songs on this Mac", subtitle: summary(songs), songs: songs, source: "downloads")
                SongTable(songs: songs, name: "Downloads", source: "downloads", query: query)
            }
            if !spoken.isEmpty && query.isEmpty {
                Divider()
                Text("Podcasts and audiobooks").font(.headline).padding(.horizontal, 16).padding(.top, 12)
                SpokenDownloads(songs: spoken)
            }
        }
        .navigationTitle("Downloads")
        .filterBox($query, rows: app.pins.count + songs.count, prompt: "Filter downloads")
    }

    private func icon(_ p: Pin) -> String {
        switch p.kind {
        case Downloads.album: "square.stack"
        case Downloads.folder: "folder"
        case Downloads.artist: "music.mic"
        case Downloads.genre: "guitars"
        case Downloads.playlist: "music.note.list"
        case Downloads.list: "heart"
        case Downloads.shelf: app.catalog.shelves[p.ref]?.isBook == false ? "mic" : "book"
        default: "music.note"
        }
    }

    private func kindLabel(_ p: Pin) -> String {
        guard p.kind == Downloads.shelf else { return p.kind.capitalized }
        return app.catalog.shelves[p.ref]?.isBook == false ? "Podcast" : "Audiobook"
    }
}
