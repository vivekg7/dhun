import DhunKit
import SwiftUI

/// Sign in, or the library window.
struct Root: View {
    @Environment(AppModel.self) private var app

    var body: some View {
        if app.signedIn {
            MainView()
        } else {
            SignInView().frame(minWidth: 820)
        }
    }
}

/// The library window: the sidebar, the list in the middle with a search
/// field, the queue or lyrics on the right, and Now playing along the bottom
/// with the hand-off bar above it (plans 024, 028).
///
/// The queue and lyrics are a panel of our own beside the split view, not
/// SwiftUI's inspector: beside a split view whose page changed, the
/// inspector looped its layout until AppKit aborted the app ("more Update
/// Constraints in Window passes than there are views"), whatever it held.
/// Inside the detail column the panel would go with each page pushed. The
/// window widens to fit it rather than squeezing the list.
struct MainView: View {
    @Environment(AppModel.self) private var app
    @Environment(Nav.self) private var nav
    @FocusState private var searching: Bool

    var body: some View {
        @Bindable var nav = nav
        VStack(spacing: 0) {
            HStack(spacing: 0) {
                NavigationSplitView {
                    Sidebar()
                        .navigationSplitViewColumnWidth(min: 190, ideal: 220, max: 300)
                } detail: {
                    NavigationStack(path: $nav.path) {
                        Detail()
                            .navigationDestination(for: Route.self) { RouteView(route: $0) }
                    }
                    .frame(minWidth: 400)
                }
                .searchable(text: $nav.search, placement: .toolbar, prompt: "Title, album or artist")
                .searchFocused($searching)
                .onChange(of: nav.findRequests) { searching = true }
                if nav.inspector {
                    Divider()
                    // Opened from Now playing's queue and lyrics buttons, or ⌥⌘U and ⌥⌘L.
                    Inspector()
                        .frame(width: 320)
                        .transition(.move(edge: .trailing))
                }
            }
            .animation(.smooth(duration: 0.25), value: nav.inspector)
            .frame(minWidth: nav.inspector ? 1000 : 820)
            Banners()
            NowPlayingBar()
        }
    }
}

/// The middle column: search results while searching, else the section.
struct Detail: View {
    @Environment(AppModel.self) private var app
    @Environment(Nav.self) private var nav

    var body: some View {
        if !nav.search.trimmingCharacters(in: .whitespaces).isEmpty {
            SearchResults(query: nav.search)
        } else {
            switch nav.section {
            case .queues: QueuesView()
            case .folders: FolderView(path: "")
            case .albums: AlbumsGrid(albums: app.catalog.albums, title: "Albums")
            case .artists: GroupsList(kind: .artist)
            case .genres: GroupsList(kind: .genre)
            case .list(let k): ListPage(kind: k)
            case .playlist(let id): PlaylistPage(id: app.sync.replaced[id] ?? id)
            case .downloads: DownloadsView()
            case nil: ContentUnavailableView("Pick something on the left", systemImage: "music.note.list")
            }
        }
    }
}

struct RouteView: View {
    @Environment(AppModel.self) private var app
    let route: Route

    var body: some View {
        switch route {
        case .album(let id):
            if let a = app.catalog.albums.first(where: { $0.id == id }) {
                AlbumPage(album: a)
            } else {
                Missing()
            }
        case .artist(let name):
            if let g = app.catalog.artists.first(where: { $0.name.lowercased() == name.lowercased() }) {
                ArtistPage(group: g)
            } else {
                Missing()
            }
        case .genre(let name):
            if let g = app.catalog.genres.first(where: { $0.name.lowercased() == name.lowercased() }) {
                SongsPage(
                    title: g.name, subtitle: summary(g.songs), songs: albumOrder(g.songs),
                    source: "genre:\(g.name)",
                    pin: (Downloads.genre, g.name))
            } else {
                Missing()
            }
        case .folder(let path): FolderView(path: path)
        case .queue(let id):
            if let q = app.queues.first(where: { $0.id == id }) {
                QueueSongs(queue: q).navigationTitle(q.name)
            } else {
                ContentUnavailableView("This queue was removed", systemImage: "list.number")
            }
        }
    }
}

struct Missing: View {
    var body: some View {
        ContentUnavailableView("No longer in the library", systemImage: "questionmark.folder")
    }
}

/// The phone's banners, above Now playing: hand-off, "continue from…?", notes.
struct Banners: View {
    @Environment(AppModel.self) private var app

    var body: some View {
        VStack(spacing: 0) {
            if let np = app.playback.handoff, let s = app.catalog.byId[np.song] {
                Banner(icon: "arrow.triangle.2.circlepath") {
                    let from = np.deviceName.isEmpty ? "another device" : np.deviceName
                    let at = clock(Double(app.playback.placeOf(np)) / 1000)
                    Text("Continue from \(from) — \(Text(s.title).bold()), \(at)")
                } actions: {
                    Button("Continue") { app.playback.continueFrom(np) }.keyboardShortcut(.defaultAction)
                    Button("Dismiss", systemImage: "xmark") { app.playback.dismissHandoff(np) }
                        .labelStyle(.iconOnly).buttonStyle(.borderless)
                }
            }
            if let (s, ms) = app.playback.offerResume {
                Banner(icon: "play.circle") {
                    Text("Continue “\(s.title)” from \(clock(Double(ms) / 1000))?")
                } actions: {
                    Button("Continue") { app.playback.answerResume(true) }
                    Button("Start over") { app.playback.answerResume(false) }
                }
            }
            if let n = app.playback.notice {
                Banner(icon: "info.circle") {
                    Text(n)
                } actions: {
                    Button("OK") { app.playback.notice = nil }
                }
            }
        }
    }
}

struct Banner<Message: View, Actions: View>: View {
    let icon: String
    @ViewBuilder let message: Message
    @ViewBuilder let actions: Actions

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: icon).foregroundStyle(.tint)
            message.lineLimit(1)
            Spacer()
            actions
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 7)
        .background(.tint.opacity(0.12))
        .overlay(alignment: .top) { Divider() }
    }
}
