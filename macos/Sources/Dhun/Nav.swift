import DhunKit
import Observation
import SwiftUI

/// What the sidebar shows: the phone's tabs, in Mac form (plan 024).
enum Section: Hashable {
    case queues
    case folders, albums, artists, genres
    /// Every audiobook, or every podcast show (plan 031).
    case books, shows
    case list(ListKind)
    case playlist(Int)
    case downloads
}

/// A page inside a section, as the phone's per-tab page stacks.
enum Route: Hashable {
    case album(String)
    case artist(String)
    case genre(String)
    case folder(String)
    case queue(String)
    /// From search results; the sidebar opens a playlist as a section.
    case playlist(Int)
    /// One book or show, by its `Shelf.id`.
    case shelf(String)
}

/// The automatic views and special lists (plan 010).
enum ListKind: String, CaseIterable, Hashable {
    case favorites, listenLater, continueListening, recentlyAdded, recentlyPlayed, mostPlayed, notLately

    var title: String {
        switch self {
        case .favorites: "Favorites"
        case .listenLater: "Listen Later"
        case .continueListening: "Continue listening"
        case .recentlyAdded: "Recently added"
        case .recentlyPlayed: "Recently played"
        case .mostPlayed: "Most played"
        case .notLately: "Not played lately"
        }
    }

    var icon: String {
        switch self {
        case .favorites: "heart"
        case .listenLater: "clock"
        case .continueListening: "play.circle"
        case .recentlyAdded: "sparkles"
        case .recentlyPlayed: "clock.arrow.circlepath"
        case .mostPlayed: "chart.bar"
        case .notLately: "hourglass"
        }
    }

    /// The queue source the server logs with each listen (plan 008).
    var source: String {
        switch self {
        case .favorites: "favorites"
        case .listenLater: "listen_later"
        case .continueListening: "continue"
        case .recentlyAdded: "recently_added"
        case .recentlyPlayed: "recently_played"
        case .mostPlayed: "most_played"
        case .notLately: "not_played_lately"
        }
    }

    /// The mark kind for the two lists the user edits.
    var mark: String? { self == .favorites ? "fav" : self == .listenLater ? "later" : nil }

    @MainActor func songs(_ app: AppModel) -> [Song] {
        let c = app.catalog
        /// The played songs `keep` takes, `first` first, at most a view's worth.
        func top(_ keep: (PlayStat) -> Bool, _ first: (PlayStat, PlayStat) -> Bool) -> [Song] {
            let ids = app.playStats.values.filter(keep).sorted(by: first).map(\.song)
            return Array(c.songsOf(ids).prefix(Catalog.viewSize))
        }
        switch self {
        case .favorites: return c.songsOf(app.favorites.map(\.song))
        case .listenLater: return c.songsOf(app.listenLater.map(\.song))
        case .continueListening: return c.songsOf(app.resumes.map(\.song))
        case .recentlyAdded: return c.recentlyAdded
        case .recentlyPlayed: return top({ $0.lastPlayedAt > 0 }, { $0.lastPlayedAt > $1.lastPlayedAt })
        case .mostPlayed: return top({ $0.count > 0 }, { $0.count > $1.count })
        case .notLately:
            // Played at least 3 times, and not in the last 90 days: favourites you have drifted away from.
            let cutoff = nowMs() - 90 * 86_400_000
            return top({ $0.count >= 3 && $0.lastPlayedAt < cutoff }, { $0.count > $1.count })
        }
    }
}

@MainActor @Observable
final class Nav {
    var section: Section? = .albums
    var path: [Route] = []
    var search = ""
    /// Bumped by Find (⌘F): the window focuses its search field.
    var findRequests = 0
    var inspector = false
    var inspectorTab = InspectorTab.queue

    enum InspectorTab: Hashable { case queue, lyrics }

    func open(_ s: Section, _ route: Route? = nil) {
        section = s
        path = route.map { [$0] } ?? []
        search = ""
    }

    /// The song's album, or the book or show of an episode or a book's file.
    func goToAlbum(_ s: Song, _ app: AppModel) {
        if let shelf = app.catalog.shelfOf(s) {
            return open(shelf.isBook ? .books : .shows, .shelf(shelf.id))
        }
        guard let a = app.catalog.albumOf[s.id].flatMap({ app.catalog.albumsById[$0] }) else { return }
        open(.albums, .album(a.id))
    }

    func goToArtist(_ s: Song) { open(.artists, .artist(s.artists.first ?? s.displayArtist)) }

    /// The right-hand panel on `tab`. `toggling`, as Now playing's buttons do, hides it if it shows that tab.
    func show(_ tab: InspectorTab, toggling: Bool = false) {
        let showing = inspector && inspectorTab == tab
        inspectorTab = tab
        inspector = !(toggling && showing)
    }
}
