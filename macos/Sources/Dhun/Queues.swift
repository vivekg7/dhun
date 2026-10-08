import DhunKit
import SwiftUI

/// The queues (plan 020), as on the phone: numbered from 1 in the user's
/// order and dragged to reorder; a click opens the queue's songs as a page,
/// with Play or Resume, Sort, Save as playlist, Rename and Remove.
///
/// A page, not the queues and their songs side by side: with the inspector
/// open, two lists side by side in the window's split view looped its size
/// updates until AppKit aborted the app.
struct QueuesView: View {
    @Environment(AppModel.self) private var app

    var body: some View {
        let app = self.app
        let queues = app.playback.ordered
        List {
            ForEach(Array(queues.enumerated()), id: \.element.id) { i, q in
                NavigationLink(value: Route.queue(q.id)) {
                    HStack {
                        Text("\(i + 1)").monospacedDigit().foregroundStyle(.secondary).frame(
                            width: 20, alignment: .trailing)
                        VStack(alignment: .leading) {
                            Text(q.name).lineLimit(1)
                            Text("\(q.songs.count) songs").font(.caption).foregroundStyle(.secondary)
                        }
                        Spacer()
                        if q.id == app.playback.activeId {
                            Image(systemName: app.playback.isPlaying ? "speaker.wave.2.fill" : "speaker.fill")
                                .foregroundStyle(.tint)
                        }
                    }
                }
                .contextMenu { QueueMenu(queue: q) }
            }
            .onMove { from, to in
                guard let f = from.first else { return }
                app.playback.moveQueue(from: f, to: to > f ? to - 1 : to)
            }
        }
        .overlay {
            if queues.isEmpty {
                ContentUnavailableView(
                    "No queues", systemImage: "list.number",
                    description: Text(
                        "Playing from any list starts a new queue; up to \(Playback.maxQueues) are kept."))
            }
        }
        .navigationTitle("Queues")
    }
}

struct QueueMenu: View {
    @Environment(AppModel.self) private var app
    let queue: QueueRow

    var body: some View {
        Button(queue.id == app.playback.activeId ? "Play" : "Switch to This Queue") {
            if queue.id == app.playback.activeId {
                app.playback.play()
            } else {
                app.playback.switchTo(queue.id)
            }
        }
        Button("Rename…") {
            Prompt.name("Rename queue", initial: queue.name) { app.playback.rename(queue.id, $0) }
        }
        Button("Save as Playlist…") {
            Prompt.name("Save as playlist", initial: queue.name) { app.store.createPlaylist($0, queue.songs) }
        }
        Divider()
        Button("Remove Queue") { app.playback.delete(queue.id) }
        Button("Remove All Other Queues") {
            if Prompt.confirm("Remove every queue but “\(queue.name)”?", action: "Remove") {
                app.playback.deleteOthers(keep: queue.id)
            }
        }
    }
}

struct QueueSongs: View {
    @Environment(AppModel.self) private var app
    @Environment(Nav.self) private var nav
    let queue: QueueRow
    @State private var selection = Set<Int>()

    var body: some View {
        // Values, not the environment, for the rows and menus built later (see `Thumb`).
        let app = self.app
        let nav = self.nav
        let isActive = queue.id == app.playback.activeId
        let ids = isActive ? app.playback.items : queue.songs
        let songs = ids.compactMap { app.catalog.byId[$0] }
        let currentId = isActive ? app.playback.current?.id : queue.currentSong
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                VStack(alignment: .leading) {
                    Text(queue.name).font(.title3.weight(.semibold)).lineLimit(1)
                    Text(
                        "\(songs.count) songs · \(clock(Double(songs.reduce(0) { $0 + $1.durationMs }) / 1000))"
                    )
                    .font(.caption).foregroundStyle(.secondary)
                }
                Spacer()
                if isActive {
                    Button(
                        app.playback.isPlaying ? "Pause" : "Play",
                        systemImage: app.playback.isPlaying ? "pause.fill" : "play.fill"
                    ) {
                        app.playback.toggle()
                    }
                } else {
                    Button("Resume", systemImage: "play.fill") { app.playback.switchTo(queue.id) }
                }
                Menu("Sort", systemImage: "arrow.up.arrow.down") {
                    ForEach(QueueSort.allCases, id: \.self) { s in
                        Button(s.label) { app.playback.reorder(queue.id, s.apply(songs).map(\.id)) }
                    }
                }
                .disabled(songs.count < 2)
                .fixedSize()
                QueueMenuButton(queue: queue)
            }
            .padding(12)
            Divider()
            ScrollViewReader { proxy in
                List(selection: $selection) {
                    ForEach(Array(songs.enumerated()), id: \.element.id) { i, s in
                        HStack(spacing: 8) {
                            Text("\(i + 1)").monospacedDigit().font(.caption).foregroundStyle(.secondary)
                                .frame(width: 28, alignment: .trailing)
                            Thumb(app: app, song: s, size: 26)
                            VStack(alignment: .leading, spacing: 1) {
                                Text(s.title).lineLimit(1).fontWeight(
                                    s.id == currentId ? .semibold : .regular
                                )
                                .foregroundStyle(
                                    s.id == currentId ? AnyShapeStyle(.tint) : AnyShapeStyle(.primary))
                                Text(s.displayArtist).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                            }
                            Spacer()
                            Text(clock(Double(s.durationMs) / 1000)).font(.caption).monospacedDigit()
                                .foregroundStyle(.secondary)
                        }
                        .id(s.id)
                        .tag(s.id)
                    }
                    .onMove { from, to in
                        guard let f = from.first else { return }
                        app.playback.moveIn(queue.id, from: f, to: to > f ? to - 1 : to)
                    }
                }
                .contextMenu(forSelectionType: Int.self) { chosen in
                    let picked = songs.filter { chosen.contains($0.id) }
                    Group {
                        if isActive, picked.count == 1, let s = picked.first {
                            Button("Stop After This Song") { app.playback.sleep.afterSong(s.id, s.title) }
                            Divider()
                        }
                        SongMenu(songs: picked)
                        Divider()
                        Button("Remove from Queue") { app.playback.removeFrom(queue.id, chosen) }
                    }
                    .environment(app)
                    .environment(nav)
                } primaryAction: { chosen in
                    guard let id = chosen.first else { return }
                    if isActive, let i = app.playback.items.firstIndex(of: id) {
                        app.playback.playAt(i)
                    } else {
                        var q = queue
                        q.currentSong = id
                        q.positionMs = 0
                        app.store.setCurrent(q)
                        app.playback.switchTo(queue.id)
                    }
                }
                .onDeleteCommand { app.playback.removeFrom(queue.id, selection) }
                .onAppear { if let currentId { proxy.scrollTo(currentId, anchor: .center) } }
                .id(queue.id)
            }
        }
    }
}

struct QueueMenuButton: View {
    let queue: QueueRow
    var body: some View {
        Menu {
            QueueMenu(queue: queue)
        } label: {
            Image(systemName: "ellipsis.circle")
        }
        .menuStyle(.borderlessButton)
        .fixedSize()
    }
}

/// How a queue can be sorted: Musicolet's Randomize and Reverse, then by what a song's tags say.
enum QueueSort: CaseIterable {
    case randomize, reverse, title, titleDesc, artist, artistDesc, album, albumDesc, folder, year, yearDesc,
        shortest,
        longest, added

    var label: String {
        switch self {
        case .randomize: "Randomize"
        case .reverse: "Reverse"
        case .title: "Title, A to Z"
        case .titleDesc: "Title, Z to A"
        case .artist: "Artist, A to Z"
        case .artistDesc: "Artist, Z to A"
        case .album: "Album, A to Z"
        case .albumDesc: "Album, Z to A"
        case .folder: "Folder and file name"
        case .year: "Year, oldest first"
        case .yearDesc: "Year, newest first"
        case .shortest: "Shortest first"
        case .longest: "Longest first"
        case .added: "Recently added first"
        }
    }

    func apply(_ s: [Song]) -> [Song] {
        let byTitle: (Song, Song) -> Bool = {
            $0.title.localizedCaseInsensitiveCompare($1.title) == .orderedAscending
        }
        let byArtist: (Song, Song) -> Bool = { a, b in
            let c = a.displayArtist.localizedCaseInsensitiveCompare(b.displayArtist)
            return c != .orderedSame ? c == .orderedAscending : byTitle(a, b)
        }
        let byAlbum: (Song, Song) -> Bool = { a, b in
            let c = a.album.localizedCaseInsensitiveCompare(b.album)
            return c != .orderedSame ? c == .orderedAscending : (a.disc, a.track) < (b.disc, b.track)
        }
        switch self {
        case .randomize: return s.shuffled()
        case .reverse: return s.reversed()
        case .title: return s.sorted(by: byTitle)
        case .titleDesc: return s.sorted(by: byTitle).reversed()
        case .artist: return s.sorted(by: byArtist)
        case .artistDesc: return s.sorted(by: byArtist).reversed()
        case .album: return s.sorted(by: byAlbum)
        case .albumDesc: return s.sorted(by: byAlbum).reversed()
        case .folder:
            return s.sorted { $0.path.localizedCaseInsensitiveCompare($1.path) == .orderedAscending }
        case .year: return s.sorted { $0.year < $1.year }
        case .yearDesc: return s.sorted { $0.year > $1.year }
        case .shortest: return s.sorted { $0.durationMs < $1.durationMs }
        case .longest: return s.sorted { $0.durationMs > $1.durationMs }
        case .added: return s.sorted { $0.addedAt > $1.addedAt }
        }
    }
}

/// A playlist (plan 013): the user's own, or a shared one the admin edits.
/// Entries that match no song are kept, shown as unavailable.
struct PlaylistPage: View {
    @Environment(AppModel.self) private var app
    @Environment(Nav.self) private var nav
    let id: Int
    @State private var selection = Set<Int>()

    var body: some View {
        // Values, not the environment, for the rows and menus built later (see `Thumb`).
        let app = self.app
        let nav = self.nav
        if let p = app.playlists.first(where: { $0.id == id }) {
            let editable = !p.shared || app.admin
            let entries = Array(p.songs.enumerated())
            let songs = app.catalog.songsOf(p.songs)
            VStack(spacing: 0) {
                ListHeader(
                    title: p.name, subtitle: (p.shared ? "Shared · " : "") + "\(songs.count) songs",
                    songs: songs,
                    source: "playlist:\(p.id)", song: songs.first,
                    pin: p.id > 0 ? (Downloads.playlist, String(p.id)) : nil
                ) {
                    if editable {
                        Menu("Edit", systemImage: "pencil") {
                            Button("Rename…") {
                                Prompt.name("Rename playlist", initial: p.name) {
                                    app.store.renamePlaylist(p, $0)
                                }
                            }
                            Menu("Sort") {
                                ForEach(QueueSort.allCases, id: \.self) { s in
                                    Button(s.label) {
                                        let unmatched = p.songs.filter { app.catalog.byId[$0] == nil }
                                        app.store.replacePlaylist(p, s.apply(songs).map(\.id) + unmatched)
                                    }
                                }
                            }
                            Divider()
                            Button("Delete Playlist…") {
                                if Prompt.confirm(
                                    "Delete “\(p.name)”?", "The server keeps a copy of the file.",
                                    action: "Delete")
                                {
                                    app.store.deletePlaylist(p)
                                    nav.open(.albums)
                                }
                            }
                        }
                        .fixedSize()
                    }
                }
                List(selection: $selection) {
                    ForEach(entries, id: \.offset) { i, songId in
                        if let s = app.catalog.byId[songId] {
                            HStack(spacing: 8) {
                                Thumb(app: app, song: s, size: 26)
                                VStack(alignment: .leading, spacing: 1) {
                                    Text(s.title).lineLimit(1)
                                    Text(s.displayArtist).font(.caption).foregroundStyle(.secondary)
                                        .lineLimit(1)
                                }
                                Spacer()
                                Text(clock(Double(s.durationMs) / 1000)).font(.caption).monospacedDigit()
                                    .foregroundStyle(.secondary)
                            }
                            .tag(i)
                        } else {
                            Text("Unavailable").foregroundStyle(.secondary).italic().tag(i)
                        }
                    }
                    .onMove { from, to in
                        guard editable, let f = from.first else { return }
                        app.store.moveInPlaylist(p, from: f, to: to > f ? to - 1 : to)
                    }
                }
                .contextMenu(forSelectionType: Int.self) { rows in
                    let picked = rows.sorted().compactMap { app.catalog.byId[p.songs[$0]] }
                    Group {
                        SongMenu(songs: picked, queueName: p.name, source: "playlist:\(p.id)", all: songs)
                        if editable {
                            Divider()
                            Button("Remove from Playlist") { remove(p, rows) }
                        }
                    }
                    .environment(app)
                    .environment(nav)
                } primaryAction: { rows in
                    guard let r = rows.first, let s = app.catalog.byId[p.songs[r]],
                        let i = songs.firstIndex(of: s)
                    else { return }
                    app.playback.play(name: p.name, source: "playlist:\(p.id)", songs: songs, start: i)
                }
                .onDeleteCommand { if editable { remove(p, selection) } }
                .id(p.id)
            }
            .navigationTitle(p.name)
        } else {
            Missing()
        }
    }

    /// From the end, so the indexes still point at the right entries.
    private func remove(_ p: Playlist, _ rows: Set<Int>) {
        var cur = p
        for r in rows.sorted(by: >) {
            app.store.removeFromPlaylist(cur, at: r)
            cur = app.playlists.first { $0.id == p.id } ?? cur
        }
        selection = []
    }
}
