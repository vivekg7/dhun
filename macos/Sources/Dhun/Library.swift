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
    let albums: [Album]
    let title: String
    @State private var filter = ""

    var body: some View {
        let shown =
            filter.isEmpty
            ? albums
            : albums.filter { fold($0.name).contains(fold(filter)) || fold($0.artist).contains(fold(filter)) }
        ScrollView {
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 150, maximum: 190), spacing: 18)], spacing: 22) {
                ForEach(shown) { a in
                    NavigationLink(value: Route.album(a.id)) {
                        VStack(alignment: .leading, spacing: 5) {
                            Cover(song: a.songs.first, size: 150)
                            HStack(spacing: 4) {
                                Text(a.name).font(.callout.weight(.medium)).lineLimit(1)
                                PinnedMark(app: app, kind: Downloads.album, ref: a.id)
                            }
                            Text(a.artist).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                        }
                        .frame(width: 150)
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .contextMenu {
                        Button("Play") {
                            app.playback.play(name: a.name, source: "album:\(a.id)", songs: a.songs, start: 0)
                        }
                        SongMenu(songs: a.songs)
                        PinButton(kind: Downloads.album, ref: a.id, name: a.name)
                    }
                }
            }
            .padding(20)
        }
        .overlay { if albums.isEmpty { Empty() } }
        .navigationTitle(title)
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

/// The phone has a search box on every list; here it filters the list in
/// place. In the toolbar, not the page: the window's first text field
/// takes the focus, and Space must play and pause, not type.
struct FilterField: ToolbarContent {
    @Binding var text: String
    let prompt: String
    var body: some ToolbarContent {
        ToolbarItem(placement: .principal) {
            TextField(prompt, text: $text).textFieldStyle(.roundedBorder).frame(width: 200)
        }
    }
}

struct AlbumPage: View {
    let album: Album

    var body: some View {
        VStack(spacing: 0) {
            ListHeader(
                title: album.name,
                subtitle: [album.artist, album.year > 0 ? String(album.year) : ""].filter { !$0.isEmpty }
                    .joined(separator: " · ") + " · " + summary(album.songs),
                songs: album.songs, source: "album:\(album.id)", song: album.songs.first,
                pin: (Downloads.album, album.id))
            SongTable(
                songs: album.songs, name: album.name, source: "album:\(album.id)", showTrack: true,
                showAlbum: false)
        }
        .navigationTitle(album.name)
    }
}

struct ArtistPage: View {
    @Environment(AppModel.self) private var app
    let group: DhunKit.Group

    var body: some View {
        let albums = app.catalog.albums.filter { a in
            a.songs.contains { s in group.songs.contains { $0.id == s.id } }
        }
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
            SongTable(songs: songs, name: group.name, source: "artist:\(group.name)")
        }
        .navigationTitle(group.name)
    }
}

/// An artist's or genre's songs by album, then disc and track: how they were released, as on the phone.
func albumOrder(_ songs: [Song]) -> [Song] { QueueSort.album.apply(songs) }

/// "12 songs · 48:10"
func summary(_ songs: [Song]) -> String {
    "\(count(songs.count)) · \(clock(Double(songs.reduce(0) { $0 + $1.durationMs }) / 1000))"
}

/// "1 song", "12 songs"
func count(_ n: Int) -> String { n == 1 ? "1 song" : "\(n) songs" }

/// A download's mark on an album, folder, artist or genre, as the phone shows.
struct PinnedMark: View {
    let app: AppModel
    let kind: String
    let ref: String

    var body: some View {
        if app.downloads.pinned(kind, ref) {
            Image(systemName: "arrow.down.circle.fill").font(.caption2).foregroundStyle(.secondary)
                .help("Downloaded")
        }
    }
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
        let shown = filter.isEmpty ? all : all.filter { fold($0.name).contains(fold(filter)) }
        List(shown) { g in
            NavigationLink(value: kind == .artist ? Route.artist(g.name) : Route.genre(g.name)) {
                HStack(spacing: 10) {
                    Thumb(app: app, song: g.songs[0], size: 32)
                    VStack(alignment: .leading) {
                        HStack(spacing: 4) {
                            Text(g.name)
                            PinnedMark(
                                app: app, kind: kind == .artist ? Downloads.artist : Downloads.genre,
                                ref: g.name)
                        }
                        Text(count(g.songs.count)).font(.caption).foregroundStyle(.secondary)
                    }
                }
            }
            .contextMenu {
                Button("Play") {
                    app.playback.play(
                        name: g.name, source: "\(kind == .artist ? "artist" : "genre"):\(g.name)",
                        songs: g.songs, start: 0)
                }
                SongMenu(songs: g.songs)
                PinButton(
                    kind: kind == .artist ? Downloads.artist : Downloads.genre, ref: g.name, name: g.name)
            }
        }
        .overlay { if all.isEmpty { Empty() } }
        .navigationTitle(kind == .artist ? "Artists" : "Genres")
        .toolbar { FilterField(text: $filter, prompt: kind == .artist ? "Filter artists" : "Filter genres") }
    }
}

/// The real NAS folder tree: subfolders, then the folder's own songs.
struct FolderView: View {
    @Environment(AppModel.self) private var app
    let path: String

    var body: some View {
        if let f = app.catalog.folders[path] {
            let app = self.app
            let name = f.path.isEmpty ? "Folders" : f.name
            VStack(spacing: 0) {
                // A folder's header, even one holding only folders: it plays and downloads them all.
                if !f.path.isEmpty {
                    ListHeader(
                        title: name, subtitle: summary(f.allSongs()), songs: f.allSongs(),
                        source: "folder:\(f.path)",
                        pin: (Downloads.folder, f.path))
                }
                if !f.children.isEmpty {
                    List(f.children) { c in
                        NavigationLink(value: Route.folder(c.path)) {
                            HStack(spacing: 4) {
                                Label(c.name, systemImage: "folder")
                                PinnedMark(app: app, kind: Downloads.folder, ref: c.path)
                            }
                        }
                        .contextMenu {
                            Button("Play") {
                                app.playback.play(
                                    name: c.name, source: "folder:\(c.path)", songs: c.allSongs(), start: 0)
                            }
                            SongMenu(songs: c.allSongs())
                            PinButton(kind: Downloads.folder, ref: c.path, name: c.name)
                        }
                    }
                    .frame(minHeight: 120, idealHeight: f.songs.isEmpty ? .infinity : 220)
                }
                if !f.songs.isEmpty {
                    if !f.children.isEmpty { Divider() }
                    SongTable(songs: f.songs, name: name, source: "folder:\(f.path)")
                }
            }
            .navigationTitle(name)
        } else {
            Empty()
        }
    }
}

/// Favorites, Listen Later and the automatic views (plan 010).
struct ListPage: View {
    @Environment(AppModel.self) private var app
    let kind: ListKind

    var body: some View {
        let songs = kind.songs(app)
        VStack(spacing: 0) {
            ListHeader(
                title: kind.title, subtitle: summary(songs), songs: songs, source: kind.source,
                pin: kind.mark.map { (Downloads.list, $0) })
            SongTable(
                songs: songs, name: kind.title, source: kind.source,
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
    }
}

struct DownloadsView: View {
    @Environment(AppModel.self) private var app

    var body: some View {
        let st = app.downloads.status
        let songs = app.catalog.songsOf(Array(app.downloaded.keys)).sorted {
            $0.title.localizedCaseInsensitiveCompare($1.title) == .orderedAscending
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
            List(app.pins) { p in
                HStack {
                    Image(systemName: icon(p.kind)).foregroundStyle(.tint).frame(width: 22)
                    VStack(alignment: .leading) {
                        Text(p.name)
                        Text(p.kind.capitalized).font(.caption).foregroundStyle(.secondary)
                    }
                    Spacer()
                    Button("Remove Download", systemImage: "xmark.circle") {
                        if confirmUnpin(p.name) { app.downloads.unpin(p.key) }
                    }
                    .labelStyle(.iconOnly).buttonStyle(.borderless).help("Remove the download")
                }
            }
            .frame(maxHeight: songs.isEmpty ? .infinity : 220)
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
                SongTable(songs: songs, name: "Downloads", source: "downloads")
            }
        }
        .navigationTitle("Downloads")
    }

    private func icon(_ kind: String) -> String {
        switch kind {
        case Downloads.album: "square.stack"
        case Downloads.folder: "folder"
        case Downloads.artist: "music.mic"
        case Downloads.genre: "guitars"
        case Downloads.playlist: "music.note.list"
        case Downloads.list: "heart"
        default: "music.note"
        }
    }
}
