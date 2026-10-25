import DhunKit
import SwiftUI

/// A table of songs: double-click or Return plays from that song in a new
/// queue named after the list (AGENTS.md: never the current queue); a right
/// click has the phone's song menu. Songs not on this Mac are dimmed while
/// the server cannot be reached.
struct SongTable: View {
    @Environment(AppModel.self) private var app
    @Environment(Nav.self) private var nav
    let songs: [Song]
    /// The queue's name when played, and the listen's source (plan 008).
    let name: String
    let source: String
    var showTrack = false
    var showAlbum = true
    /// The page's filter: the rows shown, while a song plays the whole list from there.
    var query = ""
    /// Extra menu items for this list (remove from a playlist, from Favorites).
    var extraMenu: ((Set<Int>) -> AnyView)?
    @State private var selection = Set<Int>()
    @State private var sort: [KeyPathComparator<Song>] = []

    var body: some View {
        // Values, not the environment, for the rows and menus built later (see `Thumb`).
        let app = self.app
        let nav = self.nav
        let all = sort.isEmpty ? songs : songs.sorted(using: sort)
        let rows = app.catalog.filter(all, query)
        let resumeAt = app.resumeAt
        Table(of: Song.self, selection: $selection, sortOrder: $sort) {
            if showTrack {
                TableColumn("#", value: \.track) { s in
                    Text(s.track > 0 ? "\(s.track)" : "").foregroundStyle(.secondary).monospacedDigit()
                }
                .width(28)
            }
            TableColumn("Title", value: \.title) { s in
                TitleCell(app: app, song: s)
                    // The rows are one line, so the bar runs along the cell's foot, past the cover.
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .overlay(alignment: .bottom) {
                        ResumeBar(positionMs: resumeAt[s.id], song: s).padding(.leading, 30).offset(y: 3)
                    }
            }
            .width(min: 160, ideal: 300)
            TableColumn("Artist", value: \.displayArtist) { s in Text(s.displayArtist).lineLimit(1) }
                .width(min: 90, ideal: 170)
            if showAlbum {
                TableColumn("Album", value: \.album) { s in Text(s.album).lineLimit(1) }.width(
                    min: 90, ideal: 170)
            }
            TableColumn("Time", value: \.durationMs) { s in
                Text(duration(s)).monospacedDigit().foregroundStyle(.secondary)
            }
            .width(52)
        } rows: {
            ForEach(rows) { s in
                // A drag carries the row, or the whole selection when the row is in it.
                TableRow(s).itemProvider {
                    let ids = selection.contains(s.id) ? rows.map(\.id).filter(selection.contains) : [s.id]
                    return NSItemProvider(object: ids.map(String.init).joined(separator: ",") as NSString)
                }
            }
        }
        .contextMenu(forSelectionType: Int.self) { ids in
            let chosen = rows.filter { ids.contains($0.id) }
            Group {
                SongMenu(songs: chosen, queueName: name, source: source, all: all)
                if let extraMenu, !ids.isEmpty {
                    Divider()
                    extraMenu(ids)
                }
            }
            .environment(app)
            .environment(nav)
        } primaryAction: { ids in
            // The top one in the list: a set has no order.
            guard let s = rows.first(where: { ids.contains($0.id) }), let i = all.firstIndex(of: s) else {
                return
            }
            app.playback.play(name: name, source: source, songs: all, start: i)
        }
        .emptyOverlay(songs, rows, query)
        // A table of its own per list: reusing one for another list's rows is
        // where the environment went missing, and selection and sort are the
        // list's own.
        .id(source + "\u{0}" + name)
    }
}

/// A song table's title cell: its cover, the title, the downloaded mark and,
/// on the song playing, a speaker. Dimmed when it cannot play now.
struct TitleCell: View {
    let app: AppModel
    let song: Song

    var body: some View {
        HStack(spacing: 8) {
            Thumb(app: app, song: song, size: 22)
            Text(song.title).lineLimit(1)
            KeptMark(on: app.downloaded[song.id] != nil)
            if app.playback.current?.id == song.id {
                Image(systemName: app.playback.isPlaying ? "speaker.wave.2.fill" : "speaker.fill")
                    .foregroundStyle(.tint).font(.caption)
            }
        }
        .opacity(playable(app, song) ? 1 : 0.4)
    }
}

/// A small mark on a song kept on this Mac, or on an album, folder, artist
/// or genre downloaded whole (plan 012), as the phone shows.
struct KeptMark: View {
    let on: Bool

    var body: some View {
        if on {
            Image(systemName: "arrow.down.circle.fill").font(.caption2).foregroundStyle(.secondary)
                .help("Downloaded")
        }
    }
}

/// A song in a list that is not a table: a queue, a playlist, search
/// results. Its cover, title and a line under it; a number and the resume
/// bar where the list has them. Dimmed when it cannot play now.
struct SongRow: View {
    let app: AppModel
    let song: Song
    /// The line under the title; the artist unless the list says otherwise.
    var detail: String? = nil
    var number: Int? = nil
    /// The queue's current song: bold, in the app's colour.
    var current = false
    var resumeAt: Int? = nil
    var size: CGFloat = 26

    var body: some View {
        HStack(spacing: 8) {
            if let number {
                Text("\(number)").monospacedDigit().font(.caption).foregroundStyle(.secondary)
                    .frame(width: 28, alignment: .trailing)
            }
            Thumb(app: app, song: song, size: size)
            VStack(alignment: .leading, spacing: 1) {
                Text(song.title).lineLimit(1).fontWeight(current ? .semibold : .regular)
                    .foregroundStyle(.tint(if: current))
                Text(detail ?? song.displayArtist).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                if let resumeAt { ResumeBar(positionMs: resumeAt, song: song) }
            }
            Spacer()
            KeptMark(on: app.downloaded[song.id] != nil)
            Text(duration(song)).font(.caption).monospacedDigit().foregroundStyle(.secondary)
        }
        .opacity(playable(app, song) ? 1 : 0.4)
    }
}

/// How far into a long file its resume point is, as a thin bar under its
/// title, as on the phone (plan 009); nothing in the first 1%.
struct ResumeBar: View {
    let positionMs: Int?
    let song: Song

    var body: some View {
        let f = song.durationMs > 0 ? min(Double(positionMs ?? 0) / Double(song.durationMs), 1) : 0
        if f >= 0.01 {
            GeometryReader { g in
                ZStack(alignment: .leading) {
                    Capsule().fill(.tint.opacity(0.2))
                    Capsule().fill(.tint).frame(width: g.size.width * f)
                }
            }
            .frame(height: 3)
        }
    }
}

/// Whether a song can play now: the server is there, or the song is on this Mac.
@MainActor func playable(_ app: AppModel, _ s: Song) -> Bool {
    app.reachable || app.downloaded[s.id] != nil || app.cache.songs.contains(s.id)
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
            // Playlists hold music only (plan 031).
            if songs.contains(where: { !$0.spoken }) {
                Menu("Add to Playlist") {
                    ForEach(app.playlists.filter { $0.editable }) { p in
                        Button(p.name) { added(to: p) }
                    }
                    Divider()
                    Button("New Playlist…") {
                        Prompt.name("New playlist") { app.store.createPlaylist($0, songs.map(\.id)) }
                    }
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
            let spoken = songs.filter(\.spoken)
            if !spoken.isEmpty {
                let allPlayed = spoken.allSatisfy { app.played.contains($0.id) }
                // Either way it starts from the beginning next time.
                Button(allPlayed ? "Mark as Not Played" : "Mark as Played") {
                    for s in spoken {
                        app.store.mark(Store.played, s.id, !allPlayed)
                        app.store.resume(s.id, nil)
                    }
                }
            }
            Divider()
            if songs.count == 1 {
                let s = songs[0]
                if app.downloads.pinned(Downloads.song, String(s.id)) {
                    Button("Remove Download…") {
                        if confirmUnpin(s.title) {
                            app.downloads.unpin(Downloads.key(Downloads.song, String(s.id)))
                        }
                    }
                } else if app.downloaded[s.id] == nil {
                    Button("Download") { app.downloads.pin(Downloads.song, String(s.id), s.title) }
                }
                Divider()
                if let shelf = app.catalog.shelfOf(s) {
                    Button(shelf.isBook ? "Go to Book" : "Go to Show") { nav.goToAlbum(s, app) }
                } else {
                    if !s.album.isEmpty { Button("Go to Album") { nav.goToAlbum(s, app) } }
                    Button("Go to Artist") { nav.goToArtist(s) }
                }
            } else {
                Button("Download") {
                    for s in songs { app.downloads.pin(Downloads.song, String(s.id), s.title) }
                }
            }
        }
    }

    /// Adds the songs and says what happened, as the phone does: songs already there are skipped.
    private func added(to p: Playlist) {
        let ids = app.store.music(songs.map(\.id))
        let skipped = app.store.addToPlaylist(p, ids)
        let adding = Set(ids).count - skipped
        let spoken = Set(songs.filter(\.spoken).map(\.id)).count
        app.playback.notice =
            (adding == 0
                ? (skipped == 1 ? "Already in “\(p.name)”" : "All \(skipped) already in “\(p.name)”")
                : skipped > 0
                    ? "Added \(adding) to “\(p.name)” · \(skipped) already there"
                    : "Added \(count(adding)) to “\(p.name)”")
            + (spoken > 0 ? " · \(spoken) podcast or audiobook files left out" : "")
    }
}

/// Asks before a download goes, as the phone does.
@MainActor func confirmUnpin(_ name: String) -> Bool {
    Prompt.confirm(
        "Remove download?",
        "“\(name)” stays in the library. Its songs are deleted from this Mac, unless something else you downloaded has them too.",
        action: "Remove")
}

extension Playlist {
    /// Shared playlists stay read-only in the app, for every user (plan 013).
    var editable: Bool { !shared }
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
                    // Only when there is something to play, as on the phone.
                    if !songs.isEmpty {
                        Button("Play", systemImage: "play.fill") {
                            app.playback.play(name: title, source: source, songs: songs, start: 0)
                        }
                        .buttonStyle(.borderedProminent)
                        Button("Shuffle", systemImage: "shuffle") {
                            app.playback.play(name: title, source: source, songs: songs.shuffled(), start: 0)
                        }
                    }
                    // Still offered on an empty list: Favorites kept on this Mac fills as songs are added.
                    if let (kind, ref) = pin { PinButton(kind: kind, ref: ref, name: title) }
                    extra
                }
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
            let songs = app.downloads.songs(kind, ref)
            let done = songs.count { app.downloaded[$0.id] != nil }
            Button(
                done < songs.count ? "\(done)/\(songs.count)" : "Downloaded",
                systemImage: done < songs.count ? "arrow.down.circle.dotted" : "checkmark.circle"
            ) {
                if confirmUnpin(name) { app.downloads.unpin(Downloads.key(kind, ref)) }
            }
            .help("Remove the download")
        } else {
            Button("Download", systemImage: "arrow.down.circle") { app.downloads.pin(kind, ref, name) }
        }
    }
}

/// A plain page of songs with a header: an album, a genre.
struct SongsPage: View {
    let title: String
    var subtitle = ""
    let songs: [Song]
    let source: String
    var pin: (String, String)?
    var showTrack = false
    var showAlbum = true
    @State private var query = ""

    var body: some View {
        VStack(spacing: 0) {
            ListHeader(
                title: title, subtitle: subtitle, songs: songs, source: source, song: songs.first, pin: pin)
            SongTable(
                songs: songs, name: title, source: source, showTrack: showTrack, showAlbum: showAlbum,
                query: query)
        }
        .navigationTitle(title)
        .filterBox($query, rows: songs.count)
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
        while alert.runModal() == .alertFirstButtonReturn {
            let n = field.stringValue.trimmingCharacters(in: .whitespaces)
            // The two lists' names cannot name a queue or playlist, as on the phone.
            if ["favorites", "listen later"].contains(n.lowercased()) {
                alert.informativeText = "“\(n)” is the name of one of your lists."
                continue
            }
            if !n.isEmpty { done(n) }
            return
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
