import DhunKit
import Observation
import SwiftUI

/// What the sidebar shows: the phone's tabs, in Mac form (plan 024).
enum Section: Hashable {
    case queues
    case folders, albums, artists, genres
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

    /// Each automatic view shows at most this many songs, as on the phone.
    static let viewSize = 100

    @MainActor func songs(_ app: AppModel) -> [Song] {
        let c = app.catalog
        let stats = app.playStats.values
        switch self {
        case .favorites: return c.songsOf(app.favorites.map(\.song))
        case .listenLater: return c.songsOf(app.listenLater.map(\.song))
        case .continueListening: return c.songsOf(app.resumes.map(\.song))
        case .recentlyAdded: return Array(c.songs.sorted { $0.addedAt > $1.addedAt }.prefix(Self.viewSize))
        case .recentlyPlayed:
            return Array(
                c.songsOf(
                    stats.filter { $0.lastPlayedAt > 0 }.sorted { $0.lastPlayedAt > $1.lastPlayedAt }.map(
                        \.song)
                )
                .prefix(Self.viewSize))
        case .mostPlayed:
            return Array(
                c.songsOf(stats.filter { $0.count > 0 }.sorted { $0.count > $1.count }.map(\.song)).prefix(
                    Self.viewSize))
        case .notLately:
            // Played at least 3 times, and not in the last 90 days: favourites you have drifted away from.
            let cutoff = nowMs() - 90 * 86_400_000
            return Array(
                c.songsOf(
                    stats.filter { $0.count >= 3 && $0.lastPlayedAt < cutoff }.sorted { $0.count > $1.count }
                        .map(\.song)
                ).prefix(Self.viewSize))
        }
    }
}

@MainActor @Observable
final class Nav {
    var section: Section? = .albums
    var path: [Route] = []
    var search = ""
    var inspector = false
    var inspectorTab = InspectorTab.queue

    enum InspectorTab: Hashable { case queue, lyrics }

    func open(_ s: Section, _ route: Route? = nil) {
        section = s
        path = route.map { [$0] } ?? []
        search = ""
    }

    func goToAlbum(_ s: Song, _ app: AppModel) {
        guard let a = app.catalog.albums.first(where: { $0.songs.contains { $0.id == s.id } }) else { return }
        open(.albums, .album(a.id))
    }

    func goToArtist(_ s: Song) { open(.artists, .artist(s.artists.first ?? s.displayArtist)) }
}
