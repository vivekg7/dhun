import DhunKit
import SwiftUI

/// A table of songs: double-click or Return plays from that song in a new
/// queue named after the list (AGENTS.md: never the current queue); a right
/// click has the phone's song menu. Songs not on this Mac are dimmed while
/// the server cannot be reached.
struct SongTable: View {
    @Environment(AppModel.self) private var app
    let songs: [Song]
    /// The queue's name when played, and the listen's source (plan 008).
    let name: String
    let source: String
    var showTrack = false
    var showAlbum = true
    /// Extra menu items for this list (remove from a playlist, from Favorites).
    var extraMenu: ((Set<Int>) -> AnyView)?
    @State private var selection = Set<Int>()
    @State private var sort: [KeyPathComparator<Song>] = []

    var body: some View {
        let rows = sort.isEmpty ? songs : songs.sorted(using: sort)
        Table(rows, selection: $selection, sortOrder: $sort) {
            if showTrack {
                TableColumn("#", value: \.track) { s in
                    Text(s.track > 0 ? "\(s.track)" : "").foregroundStyle(.secondary).monospacedDigit()
                }
                .width(28)
            }
            TableColumn("Title", value: \.title) { s in
                HStack(spacing: 8) {
                    Thumb(song: s, size: 22)
                    Text(s.title).lineLimit(1)
                    if app.playback.current?.id == s.id {
                        Image(systemName: app.playback.isPlaying ? "speaker.wave.2.fill" : "speaker.fill")
                            .foregroundStyle(.tint).font(.caption)
                    }
                }
                .opacity(playable(s) ? 1 : 0.4)
            }
            .width(min: 160, ideal: 300)
            TableColumn("Artist", value: \.displayArtist) { s in Text(s.displayArtist).lineLimit(1) }
                .width(min: 90, ideal: 170)
            if showAlbum {
                TableColumn("Album", value: \.album) { s in Text(s.album).lineLimit(1) }.width(
                    min: 90, ideal: 170)
            }
            TableColumn("Time", value: \.durationMs) { s in
                Text(s.durationMs > 0 ? clock(Double(s.durationMs) / 1000) : "").monospacedDigit()
                    .foregroundStyle(.secondary)
            }
            .width(52)
        }
        .contextMenu(forSelectionType: Int.self) { ids in
            let chosen = rows.filter { ids.contains($0.id) }
            SongMenu(songs: chosen, queueName: name, source: source, all: rows)
            if let extraMenu, !ids.isEmpty {
                Divider()
                extraMenu(ids)
            }
        } primaryAction: { ids in
            guard let first = ids.first, let i = rows.firstIndex(where: { $0.id == first }) else { return }
            app.playback.play(name: name, source: source, songs: rows, start: i)
        }
        .draggable(selectionText: selection)
    }

    private func playable(_ s: Song) -> Bool {
        app.reachable || app.downloaded[s.id] != nil || app.cache.songs.contains(s.id)
    }
}

extension View {
    /// Selected rows drag as their ids, onto a playlist or a list in the sidebar.
    func draggable(selectionText ids: Set<Int>) -> some View {
        self.onDrag {
            NSItemProvider(object: ids.map(String.init).joined(separator: ",") as NSString)
        }
    }
}

/// The phone's song menu (plan 011): play next, add to the playing queue or
/// another, add to a playlist, the two lists, download, go to album or artist.
struct SongMenu: View {
    @Environment(AppModel.self) private var app
    @Environment(Nav.self) private var nav
    let songs: [Song]
    var queueName = ""
    var source = ""
    var all: [Song] = []

    var body: some View {
        if songs.isEmpty {
            EmptyView()
        } else {
            if !all.isEmpty, let i = all.firstIndex(where: { $0.id == songs[0].id }) {
                Button("Play") { app.playback.play(name: queueName, source: source, songs: all, start: i) }
            }
            Button("Play Next") { app.playback.playNext(songs) }
            Button("Add to Playing Queue") { app.playback.addToQueue(songs) }
            Menu("Add to Queue") {
                ForEach(app.playback.ordered) { q in
                    Button(q.name) { app.playback.addTo(q.id, songs) }
                }
                Divider()
                Button("New Queue…") {
                    Prompt.name("New queue", initial: songs.count == 1 ? songs[0].title : "") {
                        app.playback.newQueue($0, songs)
                    }
                }
            }
            Menu("Add to Playlist") {
                ForEach(app.playlists.filter { !$0.shared || app.admin }) { p in
                    Button(p.name) { app.store.addToPlaylist(p, songs.map(\.id)) }
                }
                Divider()
                Button("New Playlist…") {
                    Prompt.name("New playlist") { app.store.createPlaylist($0, songs.map(\.id)) }
                }
            }
            Divider()
            let favs = Set(app.favorites.map(\.song))
            let allFav = songs.allSatisfy { favs.contains($0.id) }
            Button(allFav ? "Remove from Favorites" : "Add to Favorites") {
                for s in songs { app.store.mark(Store.fav, s.id, !allFav) }
            }
            let later = Set(app.listenLater.map(\.song))
            let allLater = songs.allSatisfy { later.contains($0.id) }
            Button(allLater ? "Remove from Listen Later" : "Listen Later") {
                for s in songs { app.store.mark(Store.later, s.id, !allLater) }
            }
            Divider()
            if songs.count == 1 {
                let s = songs[0]
                if app.downloads.pinned(Downloads.song, String(s.id)) {
                    Button("Remove Download") {
                        app.downloads.unpin(Downloads.key(Downloads.song, String(s.id)))
                    }
                } else if app.downloaded[s.id] == nil {
                    Button("Download") { app.downloads.pin(Downloads.song, String(s.id), s.title) }
                }
                Divider()
                if !s.album.isEmpty { Button("Go to Album") { nav.goToAlbum(s, app) } }
                Button("Go to Artist") { nav.goToArtist(s) }
            } else {
                Button("Download") {
                    for s in songs { app.downloads.pin(Downloads.song, String(s.id), s.title) }
                }
            }
        }
    }
}

/// A list's own header: its name, a line under it, Play and Shuffle, Download.
struct ListHeader<Extra: View>: View {
    @Environment(AppModel.self) private var app
    let title: String
    var subtitle = ""
    let songs: [Song]
    let source: String
    var song: Song?
    var pin: (String, String)?
    @ViewBuilder var extra: Extra

    var body: some View {
        HStack(alignment: .center, spacing: 16) {
            if let song { Cover(song: song, size: 96) }
            VStack(alignment: .leading, spacing: 4) {
                Text(title).font(.title2.weight(.semibold)).lineLimit(2)
                if !subtitle.isEmpty { Text(subtitle).foregroundStyle(.secondary) }
                HStack(spacing: 8) {
                    Button("Play", systemImage: "play.fill") {
                        app.playback.play(name: title, source: source, songs: songs, start: 0)
                    }
                    .buttonStyle(.borderedProminent)
                    Button("Shuffle", systemImage: "shuffle") {
                        app.playback.play(name: title, source: source, songs: songs.shuffled(), start: 0)
                    }
                    if let (kind, ref) = pin { PinButton(kind: kind, ref: ref, name: title) }
                    extra
                }
                .disabled(songs.isEmpty)
                .padding(.top, 4)
            }
            Spacer()
        }
        .padding(16)
    }
}

extension ListHeader where Extra == EmptyView {
    init(
        title: String, subtitle: String = "", songs: [Song], source: String, song: Song? = nil,
        pin: (String, String)? = nil
    ) {
        self.init(title: title, subtitle: subtitle, songs: songs, source: source, song: song, pin: pin) {
            EmptyView()
        }
    }
}

/// Download a whole album, folder, artist, genre, playlist or list (plan 012).
struct PinButton: View {
    @Environment(AppModel.self) private var app
    let kind: String
    let ref: String
    let name: String

    var body: some View {
        if app.downloads.pinned(kind, ref) {
            Button("Downloaded", systemImage: "checkmark.circle") {
                app.downloads.unpin(Downloads.key(kind, ref))
            }
            .help("Remove the download")
        } else {
            Button("Download", systemImage: "arrow.down.circle") { app.downloads.pin(kind, ref, name) }
        }
    }
}

/// A plain page of songs with a header.
struct SongsPage: View {
    let title: String
    var subtitle = ""
    let songs: [Song]
    let source: String
    var pin: (String, String)?

    var body: some View {
        VStack(spacing: 0) {
            ListHeader(
                title: title, subtitle: subtitle, songs: songs, source: source, song: songs.first, pin: pin)
            SongTable(songs: songs, name: title, source: source)
        }
        .navigationTitle(title)
    }
}

struct SearchResults: View {
    @Environment(AppModel.self) private var app
    let query: String

    var body: some View {
        let hits = app.catalog.search(query)
        if hits.isEmpty {
            ContentUnavailableView.search(text: query)
        } else {
            SongTable(songs: hits, name: query, source: "search")
                .navigationTitle("Search")
        }
    }
}

/// A modal text prompt, for menus that cannot hold a sheet.
@MainActor
enum Prompt {
    static func name(_ title: String, initial: String = "", _ done: @escaping (String) -> Void) {
        let alert = NSAlert()
        alert.messageText = title
        let field = NSTextField(frame: NSRect(x: 0, y: 0, width: 280, height: 24))
        field.stringValue = initial
        alert.accessoryView = field
        alert.addButton(withTitle: "OK")
        alert.addButton(withTitle: "Cancel")
        alert.window.initialFirstResponder = field
        if alert.runModal() == .alertFirstButtonReturn {
            let n = field.stringValue.trimmingCharacters(in: .whitespaces)
            if !n.isEmpty { done(n) }
        }
    }

    static func confirm(_ title: String, _ detail: String = "", action: String) -> Bool {
        let alert = NSAlert()
        alert.messageText = title
        alert.informativeText = detail
        alert.addButton(withTitle: action)
        alert.addButton(withTitle: "Cancel")
        return alert.runModal() == .alertFirstButtonReturn
    }
}
