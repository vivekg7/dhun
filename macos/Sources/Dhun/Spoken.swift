import DhunKit
import SwiftUI

// Audiobooks and podcasts, each a section of the sidebar (plan 031).

/// Playing from a book makes a queue of the whole book, named after it, from
/// the file picked or the first one not played; its resume point does the
/// rest. Playing an episode makes a queue of the show's episodes from that
/// one on, those already played left out.
@MainActor func playShelf(_ app: AppModel, _ shelf: Shelf, from: Song? = nil) {
    let heard = app.heard
    let start = from ?? heard.next(shelf)
    let source = (shelf.isBook ? "book:" : "show:") + shelf.title
    let i = shelf.items.firstIndex { $0.id == start.id } ?? 0
    if shelf.isBook {
        app.playback.play(name: shelf.title, source: source, songs: shelf.items, start: i)
    } else {
        let rest = shelf.items[i...].filter { $0.id == start.id || !heard.played.contains($0.id) }
        app.playback.play(name: shelf.title, source: source, songs: rest, start: 0)
    }
}

/// Whether a show's page leaves out the episodes already played; synced, on by default.
let hidePlayedSetting = "podcasts.hidePlayed"

/// "29 books", "1 show"
private func count(_ n: Int, _ what: String) -> String { "\(n) \(what)" + (n == 1 ? "" : "s") }

/// A book's or a show's line under its title, in lists.
@MainActor private func detail(_ s: Shelf, _ heard: Heard) -> String {
    if s.isBook { return [s.author, heard.progress(s)].filter { !$0.isEmpty }.joined(separator: " · ") }
    let new = s.items.count { !heard.played.contains($0.id) }
    return count(s.items.count, "episode") + (new > 0 ? " · \(new) not played" : "")
}

/// Every book, or every show.
struct ShelvesView: View {
    @Environment(AppModel.self) private var app
    let books: Bool
    @State private var filter = ""

    var body: some View {
        // Values, not the environment, for the rows and menus built later (see `Thumb`).
        let app = self.app
        let all = books ? app.catalog.books : app.catalog.shows
        let shown =
            filter.isEmpty
            ? all : (books ? app.catalog.bookIndex : app.catalog.showIndex).filter(SearchQuery(filter))
        let heard = app.heard
        List(shown) { s in
            NavigationLink(value: Route.shelf(s.id)) {
                HStack(spacing: 10) {
                    Thumb(app: app, song: s.cover ?? s.items[0], size: 40)
                    VStack(alignment: .leading) {
                        HStack(spacing: 4) {
                            Text(s.title).lineLimit(1)
                            PinnedMark(app: app, kind: Downloads.shelf, ref: s.id)
                        }
                        Text(detail(s, heard)).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                    }
                }
            }
            .contextMenu {
                Button(heard.begun(s) ? "Continue" : "Play") { playShelf(app, s) }
                if s.isBook { PinButton(kind: Downloads.shelf, ref: s.id, name: s.title) }
            }
        }
        .overlay {
            if all.isEmpty {
                ContentUnavailableView("Nothing here yet", systemImage: books ? "book" : "mic")
            } else if shown.isEmpty {
                ContentUnavailableView.search(text: filter)
            }
        }
        .navigationTitle(books ? "Audiobooks" : "Podcasts")
        .toolbar { FilterField(text: $filter, prompt: books ? "Filter books" : "Filter podcasts") }
    }
}

/// A book, its files in reading order; or a show, its episodes newest
/// first, those played hidden unless asked for.
struct ShelfPage: View {
    @Environment(AppModel.self) private var app
    @Environment(Nav.self) private var nav
    let shelf: Shelf
    @State private var query = ""
    @State private var selection = Set<Int>()

    var body: some View {
        let app = self.app
        let nav = self.nav
        let shelf = self.shelf
        let heard = app.heard
        let hidePlayed = !shelf.isBook && app.setting(hidePlayedSetting, true)
        let all =
            shelf.isBook
            ? shelf.items : shelf.items.reversed().filter { !hidePlayed || !heard.played.contains($0.id) }
        let found = query.isEmpty ? nil : Set(SearchIndex.songs(all).filter(SearchQuery(query)).map(\.id))
        let rows = found.map { f in all.filter { f.contains($0.id) } } ?? all
        // A book's files are often titled alike but for a part number at the end.
        let part = Dictionary(
            all.enumerated().map { ($1.id, "\($0 + 1) of \(all.count)") }, uniquingKeysWith: { a, _ in a })
        VStack(spacing: 0) {
            header(heard, hidePlayed)
            Table(of: Song.self, selection: $selection) {
                TableColumn("Title") { s in
                    HStack(spacing: 8) {
                        Thumb(app: app, song: s, size: 22)
                        Text(s.title).lineLimit(1)
                        DownloadedMark(app: app, song: s)
                        if app.playback.current?.id == s.id {
                            Image(systemName: app.playback.isPlaying ? "speaker.wave.2.fill" : "speaker.fill")
                                .foregroundStyle(.tint).font(.caption)
                        }
                    }
                    .opacity(playable(app, s) ? 1 : 0.4)
                }
                .width(min: 160, ideal: 360)
                if !shelf.isBook {
                    TableColumn("Date") { s in Text(s.day).monospacedDigit().foregroundStyle(.secondary) }
                        .width(84)
                }
                if shelf.isBook && all.count > 1 {
                    TableColumn("Part") { s in
                        Text(part[s.id] ?? "").monospacedDigit().foregroundStyle(.secondary)
                    }
                    .width(64)
                }
                TableColumn("Heard") { s in Text(heard.state(s)).foregroundStyle(.secondary).lineLimit(1) }
                    .width(min: 70, ideal: 100)
                TableColumn("Time") { s in
                    Text(s.durationMs > 0 ? clock(Double(s.durationMs) / 1000) : "").monospacedDigit()
                        .foregroundStyle(.secondary)
                }
                .width(64)
            } rows: {
                ForEach(rows) { s in
                    TableRow(s).itemProvider { NSItemProvider(object: String(s.id) as NSString) }
                }
            }
            .contextMenu(forSelectionType: Int.self) { ids in
                let chosen = rows.filter { ids.contains($0.id) }
                Group {
                    if let first = chosen.first { Button("Play") { playShelf(app, shelf, from: first) } }
                    SongMenu(songs: chosen)
                }
                .environment(app)
                .environment(nav)
            } primaryAction: { ids in
                guard let s = rows.first(where: { ids.contains($0.id) }) else { return }
                playShelf(app, shelf, from: s)
            }
            .overlay {
                if all.isEmpty {
                    ContentUnavailableView("Every episode is played", systemImage: "checkmark.circle")
                } else if rows.isEmpty {
                    ContentUnavailableView.search(text: query)
                }
            }
            .id(shelf.id)
        }
        .navigationTitle(shelf.title)
        .filterBox($query, rows: all.count, prompt: shelf.isBook ? "Filter files" : "Filter episodes")
    }

    private func header(_ heard: Heard, _ hidePlayed: Bool) -> some View {
        let facts =
            shelf.isBook
            ? [shelf.author, heard.progress(shelf)]
            : [
                count(shelf.items.count, "episode"),
                "\(shelf.items.count { !heard.played.contains($0.id) }) not played",
            ]
        return HStack(alignment: .center, spacing: 16) {
            Cover(song: shelf.cover ?? shelf.items[0], size: 96)
            VStack(alignment: .leading, spacing: 4) {
                Text(shelf.title).font(.title2.weight(.semibold)).lineLimit(2)
                Text(facts.filter { !$0.isEmpty }.joined(separator: " · ")).foregroundStyle(.secondary)
                HStack(spacing: 8) {
                    Button(heard.begun(shelf) ? "Continue" : "Play", systemImage: "play.fill") {
                        playShelf(app, shelf)
                    }
                    .buttonStyle(.borderedProminent)
                    if shelf.isBook {
                        PinButton(kind: Downloads.shelf, ref: shelf.id, name: shelf.title)
                    } else {
                        Button(
                            hidePlayed ? "Show Played" : "Hide Played",
                            systemImage: hidePlayed ? "eye" : "eye.slash"
                        ) {
                            app.store.setting(hidePlayedSetting, .bool(!hidePlayed))
                        }
                    }
                }
                .padding(.top, 4)
            }
            Spacer()
        }
        .padding(16)
    }
}

/// The chapter playing in `s`, as "Chapter 2 of 20 · Name"; nil without chapters.
@MainActor func chapterLabel(_ s: Song, _ positionMs: Int) -> String? {
    guard !s.chapters.isEmpty else { return nil }
    let i = max(0, s.chapters.lastIndex { $0.startMs <= positionMs } ?? 0)
    let name = s.chapters[i].name
    return "Chapter \(i + 1) of \(s.chapters.count)" + (name.isEmpty ? "" : " · \(name)")
}

/// An episode's or a book file's chapters, each a click away, and its notes.
struct ChaptersPanel: View {
    @Environment(AppModel.self) private var app
    let song: Song
    let done: () -> Void

    var body: some View {
        let p = app.playback
        let now = song.chapters.lastIndex { $0.startMs <= Int(p.position * 1000) }
        ScrollView {
            VStack(alignment: .leading, spacing: 4) {
                Text(song.title).font(.headline).lineLimit(2).padding(.bottom, 6)
                if !song.chapters.isEmpty {
                    Text("Chapters").font(.caption.weight(.semibold)).foregroundStyle(.secondary)
                    ForEach(Array(song.chapters.enumerated()), id: \.offset) { i, ch in
                        Button {
                            p.seek(to: Double(ch.startMs) / 1000)
                            done()
                        } label: {
                            HStack(alignment: .firstTextBaseline, spacing: 10) {
                                Text(clock(Double(ch.startMs) / 1000)).monospacedDigit().foregroundStyle(
                                    .secondary
                                )
                                .frame(width: 64, alignment: .leading)
                                Text(ch.name.isEmpty ? "Chapter \(i + 1)" : ch.name)
                                    .foregroundStyle(
                                        i == now ? AnyShapeStyle(.tint) : AnyShapeStyle(.primary)
                                    )
                                    .fontWeight(i == now ? .semibold : .regular)
                                Spacer(minLength: 0)
                            }
                            .padding(.vertical, 3)
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                    }
                }
                if !song.notes.isEmpty {
                    Text("Notes").font(.caption.weight(.semibold)).foregroundStyle(.secondary).padding(
                        .top, 8)
                    Text(song.notes).textSelection(.enabled)
                }
            }
            .padding(16)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .frame(width: 380)
        .frame(maxHeight: 480)
    }
}

/// Downloaded episodes and books' files: each plays its book or show from there.
struct SpokenDownloads: View {
    @Environment(AppModel.self) private var app
    @Environment(Nav.self) private var nav
    let songs: [Song]

    var body: some View {
        let app = self.app
        let nav = self.nav
        Table(songs) {
            TableColumn("Title") { s in
                HStack(spacing: 8) {
                    Thumb(app: app, song: s, size: 22)
                    Text(s.title).lineLimit(1)
                }
            }
            .width(min: 160, ideal: 300)
            TableColumn("Book or show") { s in Text(s.groupTitle).foregroundStyle(.secondary).lineLimit(1) }
            TableColumn("Time") { s in
                Text(s.durationMs > 0 ? clock(Double(s.durationMs) / 1000) : "").monospacedDigit()
                    .foregroundStyle(.secondary)
            }
            .width(64)
        }
        .contextMenu(forSelectionType: Int.self) { ids in
            SongMenu(songs: songs.filter { ids.contains($0.id) }).environment(app).environment(nav)
        } primaryAction: { ids in
            guard let s = songs.first(where: { ids.contains($0.id) }), let shelf = app.catalog.shelfOf(s)
            else {
                return
            }
            playShelf(app, shelf, from: s)
        }
    }
}
