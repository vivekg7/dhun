import DhunKit
import SwiftUI

/// A list row's cover: the thumbnail every cover has, never waiting (plan 019).
struct Thumb: View {
    @Environment(AppModel.self) private var app
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

/// A larger cover, fetched when first shown; the thumbnail until it arrives.
struct Cover: View {
    @Environment(AppModel.self) private var app
    let song: Song?
    let size: CGFloat
    @State private var image: NSImage?

    var body: some View {
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
        .task(id: "\(song?.art ?? "")-\(song?.id ?? 0)-\(Int(size))") {
            image = nil
            guard let song else { return }
            image = await app.covers.image(song, px: Int(size * 2))
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
                            Text(a.name).font(.callout.weight(.medium)).lineLimit(1)
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

/// The phone has a search box on every list; here it filters the list in place.
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
                    .joined(separator: " · ") + " · \(album.songs.count) songs",
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
        VStack(spacing: 0) {
            ListHeader(
                title: group.name, subtitle: "\(albums.count) albums · \(group.songs.count) songs",
                songs: group.songs,
                source: "artist:\(group.name)", song: group.songs.first, pin: (Downloads.artist, group.name))
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
            SongTable(songs: group.songs, name: group.name, source: "artist:\(group.name)")
        }
        .navigationTitle(group.name)
    }
}

/// Artists or genres: a filtered list, each opening its songs.
struct GroupsList: View {
    enum Kind { case artist, genre }
    @Environment(AppModel.self) private var app
    let kind: Kind
    @State private var filter = ""

    var body: some View {
        let all = kind == .artist ? app.catalog.artists : app.catalog.genres
        let shown = filter.isEmpty ? all : all.filter { fold($0.name).contains(fold(filter)) }
        List(shown) { g in
            NavigationLink(value: kind == .artist ? Route.artist(g.name) : Route.genre(g.name)) {
                HStack(spacing: 10) {
                    Thumb(song: g.songs[0], size: 32)
                    VStack(alignment: .leading) {
                        Text(g.name)
                        Text("\(g.songs.count) songs").font(.caption).foregroundStyle(.secondary)
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
            let name = f.path.isEmpty ? "Folders" : f.name
            VStack(spacing: 0) {
                if !f.children.isEmpty {
                    List(f.children) { c in
                        NavigationLink(value: Route.folder(c.path)) {
                            Label(c.name, systemImage: "folder")
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
                    if !f.path.isEmpty {
                        ListHeader(
                            title: name, subtitle: "\(f.allSongs().count) songs", songs: f.allSongs(),
                            source: "folder:\(f.path)",
                            pin: (Downloads.folder, f.path))
                    }
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
                title: kind.title, subtitle: "\(songs.count) songs", songs: songs, source: kind.source,
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
                    Button("Remove", systemImage: "xmark.circle") { app.downloads.unpin(p.key) }
                        .labelStyle(.iconOnly).buttonStyle(.borderless)
                }
            }
            .overlay {
                if app.pins.isEmpty {
                    ContentUnavailableView(
                        "Nothing downloaded", systemImage: "arrow.down.circle",
                        description: Text(
                            "Download an album, a playlist or Favorites to play them without the server."))
                }
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
