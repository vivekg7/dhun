import Foundation
import Observation

/// The player and the queues around it, the phone's `Playback` (plans 007,
/// 024). The engine holds the songs being heard; this holds the active
/// queue's play order and loads only that queue, so switching saves the
/// outgoing queue's song and position and loads the incoming one where it
/// was left. Every listen is logged (plan 008) and long files keep a resume
/// point (plan 009).
///
/// State lives on the main actor. Engine calls go to a serial queue of their
/// own, since opening a song may wait for the network.
@MainActor @Observable
public final class Playback {
    public static let maxQueues = 20
    /// The synced setting with the queues' order: their ids, as a JSON array.
    public static let orderSetting = "queues.order"
    /// A hand-off never starts closer than this to the song's end.
    static let endMarginMs = 5000

    @ObservationIgnored unowned let app: AppModel
    @ObservationIgnored public let engine = Engine()
    @ObservationIgnored private let control = DispatchQueue(label: "dhun.control")
    @ObservationIgnored private let upcomingState = Upcoming()

    public private(set) var activeId: String
    public private(set) var current: Song?
    public private(set) var isPlaying = false
    /// Where in the current song, in seconds; follows the engine four times a second while playing.
    public private(set) var position: Double = 0
    public private(set) var decodedDuration: Double = 0
    /// Held up because the song cannot be fetched: the fetch keeps trying (plan 019).
    public private(set) var waiting = false
    /// A long file with a resume point, waiting for the user's answer ("ask", plan 009).
    public var offerResume: (song: Song, ms: Int)?
    @ObservationIgnored private var _sleep: SleepTimer!
    public var sleep: SleepTimer { _sleep }
    /// Speed and pitch (plan 016): this Mac's everyday one.
    public private(set) var everyday: Tempo
    /// A note for the user (a queue removed to make room), shown briefly.
    public var notice: String?

    /// The active queue's songs as loaded, in queue order; `order` is the
    /// play order of the same song ids, shuffled or not.
    public private(set) var items: [Int] = []
    @ObservationIgnored private var order: [Int] = []
    @ObservationIgnored private var restored = false
    @ObservationIgnored private var commands = 0
    @ObservationIgnored private var ended = false
    @ObservationIgnored private var needsRestart = false
    @ObservationIgnored private var failures = 0
    @ObservationIgnored private var open: Open?
    @ObservationIgnored private var heardSpeed = 1.0
    @ObservationIgnored private var ticks = 0
    @ObservationIgnored private var timer: Timer?
    @ObservationIgnored private var media: MediaKeys!
    private var stateAt: Int
    private var dismissed: String

    init(app: AppModel) {
        self.app = app
        activeId = app.prefs.activeQueue
        everyday = Tempo(speed: app.prefs.speed, semitones: app.prefs.semitones)
        stateAt = app.prefs.stateAt
        dismissed = app.prefs.handoffDismissed
        _sleep = SleepTimer(playback: self)
        media = MediaKeys(playback: self)
        let state = upcomingState
        engine.next = { [weak self] after in
            guard let next = state.next(after: after) else { return nil }
            let bytes: Bytes? = DispatchQueue.main.sync {
                MainActor.assumeIsolated { self?.bytes(for: next) }
            }
            return bytes.map { (next, $0) }
        }
        engine.failed = { [weak self] song, error in
            Task { @MainActor in self?.songFailed(song, error) }
        }
        engine.interrupted = { [weak self] in
            Task { @MainActor in self?.outputChanged() }
        }
        timer = Timer.scheduledTimer(withTimeInterval: 0.25, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated { self?.tick() }
        }
        closeInterrupted()
        applyTempo()
    }

    // MARK: Derived state

    public var active: QueueRow? { app.queues.first { $0.id == activeId } }

    /// The queues in the user's order, numbered from 1 as in Musicolet (plan 020).
    public var ordered: [QueueRow] { Playback.orderOf(app.queues, app.settings[Playback.orderSetting]) }

    public var shuffle: Bool { active?.shuffle ?? false }
    public var repeatMode: String { active?.repeatMode ?? "off" }

    /// The playing song's own speed and pitch, synced; nil when it follows `everyday`.
    public var songTempo: Tempo? { current.flatMap { Tempo.of(app.settings[Tempo.songSetting($0.id)]) } }
    public var tempo: Tempo { songTempo ?? everyday }

    public var duration: Double {
        if let s = current, s.durationMs > 0 { return Double(s.durationMs) / 1000 }
        return decodedDuration
    }

    var hasNext: Bool { nextIndex(after: currentIndex, repeat: repeatMode == "queue") != nil }
    private var currentIndex: Int? { current.flatMap { items.firstIndex(of: $0.id) } }

    /// "Continue from MacBook": another device's playback, offered while this
    /// one is not playing, when it is newer than anything this Mac played and
    /// its queue and song are here (plan 017).
    public var handoff: NowPlaying? {
        guard let np = app.nowPlaying, !isPlaying, app.prefs.deviceId != 0, np.deviceId != app.prefs.deviceId,
            parseTime(np.at) > stateAt, np.at != dismissed, app.queues.contains(where: { $0.id == np.queue }),
            app.catalog.byId[np.song] != nil
        else { return nil }
        return np
    }

    /// The song playing and the next `ahead` in play order, for the song cache.
    func upcoming(_ ahead: Int) -> [Song] {
        guard let i = currentIndex else { return [] }
        var out = [i]
        var at = i
        while out.count <= ahead, let n = nextIndex(after: at, repeat: repeatMode == "queue"),
            !out.contains(n)
        {
            out.append(n)
            at = n
        }
        return out.compactMap { app.catalog.byId[items[$0]] }
    }

    // MARK: Transport

    public func toggle() { isPlaying ? pause() : play() }

    public func play() {
        guard let s = current else { return }
        if ended || needsRestart {
            // The queue ran out: play its song again, as Media3 does on Play.
            start(s, at: ended ? 0 : position, paused: false)
            return
        }
        setPlaying(true)
        control.async { [engine] in try? engine.resume() }
    }

    public func pause() {
        guard isPlaying else { return }
        engine.pause()
        setPlaying(false)
    }

    public func next() {
        guard let i = currentIndex, let n = nextIndex(after: i, repeat: repeatMode == "queue") else { return }
        jump(to: n, end: "skipped")
    }

    /// Back to the start of the song, or to the one before within its first three seconds.
    public func previous() {
        guard let i = currentIndex else { return }
        if position > 3 {
            seek(to: 0)
            return
        }
        guard let p = previousIndex(before: i) else { return seek(to: 0) }
        jump(to: p, end: "previous")
    }

    public func seek(to seconds: Double) {
        guard current != nil else { return }
        position = max(0, seconds)
        defer { media.update() }
        if ended || needsRestart, let s = current {
            start(s, at: seconds, paused: !isPlaying)
            return
        }
        commands += 1
        control.async { [engine] in
            try? engine.seek(to: seconds)
            DispatchQueue.main.async { MainActor.assumeIsolated { self.commands -= 1 } }
        }
    }

    /// Plays the song at `index` of the active queue's own order.
    public func playAt(_ index: Int) {
        guard items.indices.contains(index) else { return }
        jump(to: index, end: "switched", play: true)
    }

    // MARK: Queues

    /// Plays `songs` from `start` in a new queue named `name` (AGENTS.md:
    /// playing from a list never overwrites the current queue). A queue
    /// already called `name` is reused and refilled rather than duplicated,
    /// so playing songs in one album does not leave "Album (2)", "Album (3)".
    public func play(name: String, source: String, songs: [Song], start: Int) {
        guard !songs.isEmpty else { return }
        saveActive()
        let first = songs[min(max(start, 0), songs.count - 1)]
        var seen = Set<Int>()
        let ids = songs.map(\.id).filter { seen.insert($0).inserted }
        let now = nowMs()
        let q: QueueRow
        if var same = app.queues.first(where: { $0.name.lowercased() == name.lowercased() }) {
            same.songs = ids
            same.currentSong = first.id
            same.positionMs = 0
            same.usedAt = now
            app.store.replaceQueue(same)
            app.store.setCurrent(same)
            q = same
        } else {
            q = create(name, ids, first.id, now)
        }
        app.prefs.setQueueSource(q.id, source)
        load(q, play: true)
    }

    /// A new queue, last in the order. At most `maxQueues`: as in Musicolet,
    /// the first queue goes to make room, and a note says so.
    @discardableResult
    private func create(_ name: String, _ ids: [Int], _ current: Int, _ now: Int) -> QueueRow {
        let order = ordered
        let over = max(0, order.count - (Playback.maxQueues - 1))
        for q in order.prefix(over) { app.store.deleteQueue(q.id) }
        if over > 0 { notice = "At most \(Playback.maxQueues) queues: “\(order[0].name)” was removed" }
        let q = QueueRow(
            id: UUID().uuidString.lowercased(), name: name, songs: ids, currentSong: current, positionMs: 0,
            shuffle: false, repeatMode: "off", usedAt: now)
        app.store.createQueue(q)
        setOrder(order.dropFirst(over).map(\.id) + [q.id])
        return q
    }

    /// A queue of `songs` that does not start playing. The name is made
    /// unique the way the server does it, so the two agree.
    public func newQueue(_ name: String, _ songs: [Song]) {
        let taken = Set(app.queues.map { $0.name.lowercased() })
        var unique = name
        var n = 2
        while taken.contains(unique.lowercased()) {
            unique = "\(name) (\(n))"
            n += 1
        }
        let adding = distinct(songs.filter { app.catalog.byId[$0.id] != nil }.map(\.id))
        create(unique, adding, adding.first ?? 0, nowMs())
    }

    private func setOrder(_ ids: [String]) {
        app.store.setting(Playback.orderSetting, .array(ids.map { .string($0) }))
    }

    /// Drag in the queue list: queue `from` goes to `to`, counting from 0.
    public func moveQueue(from: Int, to: Int) {
        var ids = ordered.map(\.id)
        guard ids.indices.contains(from), ids.indices.contains(to), from != to else { return }
        ids.insert(ids.remove(at: from), at: to)
        setOrder(ids)
    }

    /// Every queue but `keep` (Musicolet's "Remove all other queues").
    public func deleteOthers(keep: String) {
        for q in app.queues where q.id != keep { app.store.deleteQueue(q.id) }
        setOrder([keep])
    }

    public func switchTo(_ id: String) {
        guard id != activeId, var q = app.queues.first(where: { $0.id == id }) else { return }
        saveActive()
        q.usedAt = nowMs()
        load(q, play: true)
    }

    /// After the current song; songs already queued are moved, never duplicated.
    public func playNext(_ songs: [Song]) { insert(songs, next: true) }
    public func addToQueue(_ songs: [Song]) { insert(songs, next: false) }

    private func insert(_ songs: [Song], next: Bool) {
        guard let q = active else { return play(name: "Queue", source: "", songs: songs, start: 0) }
        let playing = current?.id
        let adding = distinct(songs.map(\.id).filter { $0 != playing && app.catalog.byId[$0] != nil })
        guard !adding.isEmpty else { return }
        items.removeAll { adding.contains($0) }
        let at = next ? (currentIndex.map { $0 + 1 } ?? 0) : items.count
        items.insert(contentsOf: adding, at: at)
        rebuildOrder(keepingShuffle: true)
        var row = q
        row.songs = items
        app.store.insertIntoQueue(row, adding, after: at == 0 ? 0 : items[at - 1])
        sleep.update()
        app.cache.poke()
    }

    /// Adds `songs` to the end of queue `id`; songs already in it move there.
    public func addTo(_ id: String, _ songs: [Song]) {
        if id == activeId { return addToQueue(songs) }
        guard var q = app.queues.first(where: { $0.id == id }) else { return }
        let adding = distinct(songs.map(\.id).filter { app.catalog.byId[$0] != nil })
        guard !adding.isEmpty else { return }
        q.songs = q.songs.filter { !adding.contains($0) } + adding
        if q.currentSong == 0 { q.currentSong = q.songs[0] }
        app.store.insertIntoQueue(q, adding, after: nil)
    }

    public func removeFrom(_ id: String, _ songs: Set<Int>) {
        guard !songs.isEmpty, var q = app.queues.first(where: { $0.id == id }) else { return }
        if id == activeId {
            let wasCurrent = current.map { songs.contains($0.id) } ?? false
            let following = currentIndex.flatMap { i in items[(i + 1)...].first { !songs.contains($0) } }
            items.removeAll { songs.contains($0) }
            rebuildOrder(keepingShuffle: true)
            q.songs = items
            app.store.removeFromQueue(q, Array(songs))
            if wasCurrent {
                // The player goes on with the song after it, as Media3 does.
                close("skipped")
                if let n = following ?? items.first, let s = app.catalog.byId[n] {
                    start(s, at: 0, paused: !isPlaying)
                } else {
                    stopAll()
                }
            }
            sleep.update()
            app.cache.poke()
            return
        }
        let old = q.songs
        q.songs = old.filter { !songs.contains($0) }
        app.store.removeFromQueue(q, Array(songs))
        if songs.contains(q.currentSong) {
            let start = (old.firstIndex(of: q.currentSong) ?? 0) + 1
            q.currentSong = old[min(start, old.count)...].first { !songs.contains($0) } ?? q.songs.first ?? 0
            q.positionMs = 0
            app.store.setCurrent(q)
        }
    }

    /// Moves song `from` of queue `id` to `to`, counting from 0 in the queue's order.
    public func moveIn(_ id: String, from: Int, to: Int) {
        guard from != to, var q = app.queues.first(where: { $0.id == id }) else { return }
        var list = id == activeId ? items : q.songs
        guard list.indices.contains(from), list.indices.contains(to) else { return }
        let song = list.remove(at: from)
        list.insert(song, at: to)
        if id == activeId {
            items = list
            rebuildOrder(keepingShuffle: true)
            sleep.update()
            app.cache.poke()
        }
        q.songs = list
        app.store.moveInQueue(q, song: song, after: to == 0 ? 0 : list[to - 1])
    }

    /// Queue `id` in a new order of the same songs (a sort, Randomize,
    /// Reverse). The playing song plays on.
    public func reorder(_ id: String, _ newOrder: [Int]) {
        guard var q = app.queues.first(where: { $0.id == id }) else { return }
        if id == activeId {
            let loaded = Set(items)
            items = newOrder.filter { loaded.contains($0) }
            rebuildOrder(keepingShuffle: true)
            q.songs = items
            sleep.update()
            app.cache.poke()
        } else {
            q.songs = newOrder
        }
        app.store.replaceQueue(q)
    }

    public func rename(_ id: String, _ name: String) {
        guard var q = app.queues.first(where: { $0.id == id }) else { return }
        q.name = name
        app.store.renameQueue(q)
    }

    public func delete(_ id: String) {
        setOrder(ordered.map(\.id).filter { $0 != id })
        if id == activeId {
            close("stopped")
            stopAll()
            app.store.deleteQueue(id)
            // The one used most recently takes its place, paused.
            if let q = app.queues.first(where: { $0.id != id }) { load(q, play: false) }
        } else {
            app.store.deleteQueue(id)
        }
    }

    public func toggleShuffle() {
        guard var q = active else { return }
        q.shuffle.toggle()
        app.store.setMode(q)
        rebuildOrder(keepingShuffle: false)
    }

    /// off → whole queue → this song → off.
    public func cycleRepeat() {
        guard var q = active else { return }
        q.repeatMode = ["off": "queue", "queue": "song"][q.repeatMode] ?? "off"
        app.store.setMode(q)
        publishUpcoming()
        sleep.update()
    }

    public func answerResume(_ accept: Bool) {
        guard let (song, ms) = offerResume else { return }
        offerResume = nil
        if accept && current?.id == song.id { seek(to: Double(ms) / 1000) }
    }

    // MARK: Speed and pitch

    /// For the playing song only (synced, as its own setting), or as this Mac's everyday one.
    public func setTempo(_ t: Tempo, onlyThisSong: Bool) {
        if onlyThisSong, let s = current {
            app.store.setting(Tempo.songSetting(s.id), t.json)
        } else {
            everyday = t
            app.prefs.speed = t.speed
            app.prefs.semitones = t.semitones
        }
        applyTempo()
    }

    /// The playing song follows the everyday speed and pitch again.
    public func clearSongTempo() {
        guard let s = current else { return }
        app.store.setting(Tempo.songSetting(s.id), .null)
        applyTempo()
    }

    func applyTempo() {
        // Time heard so far counts at the old speed.
        if let o = open { count(o) }
        let t = tempo
        heardSpeed = t.speed
        engine.speed = Float(t.speed)
        engine.semitones = Float(t.semitones)
    }

    // MARK: Hand-off (plan 017)

    /// Where a hand-off continues: the other device's place, moved on by the
    /// time passed if it was still playing, and never at the very end.
    public func placeOf(_ np: NowPlaying) -> Int {
        var pos = np.positionMs
        if np.playing { pos += max(0, nowMs() - parseTime(np.at)) }
        if let length = app.catalog.byId[np.song]?.durationMs, length > 0 {
            pos = min(pos, length - Playback.endMarginMs)
        }
        return max(0, pos)
    }

    /// Takes over from another device: its queue (queues are synced), its song, and its place.
    public func continueFrom(_ np: NowPlaying) {
        guard var q = app.queues.first(where: { $0.id == np.queue }), let song = app.catalog.byId[np.song]
        else { return }
        let pos = placeOf(np)
        if q.id != activeId { saveActive() }
        q.currentSong = song.id
        q.positionMs = pos
        q.usedAt = nowMs()
        load(q, play: true, position: pos)
    }

    /// ✕ on the offer: not offered again, though a newer playback on that device will be.
    public func dismissHandoff(_ np: NowPlaying) {
        app.prefs.handoffDismissed = np.at
        dismissed = np.at
    }

    /// What the user's devices played last, asked directly, because after a
    /// restart the sync cursor is already past it.
    func checkHandoff() async {
        guard app.signedIn else { return }
        do {
            if app.prefs.deviceId == 0 || !app.prefs.adminKnown {
                let me = try await app.api.me()
                app.prefs.deviceId = me.deviceId ?? 0
                app.prefs.admin = me.user?.admin ?? false
            }
            if let np = try await app.api.nowPlaying() { app.nowPlaying = np }
        } catch {
            // Offline, or an answer we cannot read: nothing to hand off from this time.
        }
    }

    /// Tells the server what this Mac plays, for the others' hand-off.
    private func report(_ q: QueueRow, _ s: Song, _ posMs: Int, _ playing: Bool) {
        let now = nowMs()
        app.prefs.stateAt = now
        stateAt = now
        app.store.playbackState(queue: q.id, song: s.id, positionMs: posMs, playing: playing)
    }

    /// On sign-out: the open listen belongs to the account being left, and is dropped.
    func reset() {
        sleep.cancel()
        open = nil
        app.prefs.openListen = ""
        app.nowPlaying = nil
        stateAt = 0
        dismissed = ""
        stopAll()
        offerResume = nil
        restored = true
    }

    // MARK: Loading

    /// The catalogue arrived: reload the queue that was playing when the app
    /// last stopped, paused, once.
    func catalogChanged() {
        guard !restored, !app.catalog.songs.isEmpty else { return }
        restored = true
        if current == nil, let q = active { load(q, play: false) }
    }

    func queuesSynced() { sleep.update() }

    private func load(_ q: QueueRow, play: Bool, position: Int? = nil) {
        close("switched")
        let songs = app.catalog.songsOf(q.songs)
        setActive(q.id)
        guard !songs.isEmpty else {
            stopAll()
            return
        }
        let index = songs.firstIndex { $0.id == q.currentSong } ?? 0
        let song = songs[index]
        var start = position
        if start != nil { offerResume = nil }
        if start == nil { start = startPosition(song) }
        if start == nil { start = song.id == q.currentSong ? q.positionMs : 0 }
        items = songs.map(\.id)
        rebuildOrder(keepingShuffle: false, shuffle: q.shuffle)
        app.store.putQueue(q)
        self.start(song, at: Double(start!) / 1000, paused: !play)
    }

    /// Opens `s` in the engine at `seconds`; the listen opens with it.
    private func start(_ s: Song, at seconds: Double, paused: Bool) {
        current = s
        position = seconds
        decodedDuration = 0
        ended = false
        needsRestart = false
        applyTempo()
        publishUpcoming()
        sleep.update()
        openListen(s, Int(seconds * 1000))
        guard let bytes = bytes(for: s.id) else {
            return songFailed(s.id, BytesError.failed("Not available"))
        }
        commands += 1
        let engine = self.engine
        control.async {
            var error: Error?
            do { try engine.play(song: s.id, bytes: bytes, from: seconds, paused: paused) } catch let e {
                error = e
            }
            DispatchQueue.main.async {
                MainActor.assumeIsolated {
                    self.commands -= 1
                    if let error { self.songFailed(s.id, error) }
                }
            }
        }
        setPlaying(!paused)
        app.cache.poke()
        media.update()
    }

    private func jump(to index: Int, end: String, play: Bool? = nil) {
        guard let s = app.catalog.byId[items[index]] else { return }
        let finished = current
        let at = position
        close(end, toMs: Int(at * 1000))
        if let finished { saveResume(finished, Int(at * 1000)) }
        sleep.onNextSong()
        var start = startPosition(s) ?? 0
        if s.id == finished?.id { start = 0 }
        self.start(s, at: Double(start) / 1000, paused: !(play ?? isPlaying))
        afterSongChange(s, startMs: start)
    }

    private func afterSongChange(_ s: Song, startMs: Int) {
        guard var q = active else { return }
        q.currentSong = s.id
        q.positionMs = startMs
        app.store.setCurrent(q)
        // The next song, for a hand-off from another device.
        if isPlaying { report(q, s, startMs, true) }
    }

    private func stopAll() {
        defer { media.update() }
        // Not on the main thread: stopping waits for the decoding queue,
        // which may itself be waiting on the main thread for the next song.
        control.async { [engine] in engine.stop() }
        items = []
        order = []
        current = nil
        position = 0
        setPlaying(false)
        publishUpcoming()
        if activeId != "" && !app.queues.contains(where: { $0.id == activeId }) { setActive("") }
    }

    private func setActive(_ id: String) {
        activeId = id
        app.prefs.activeQueue = id
    }

    /// Records where the outgoing queue was, before another is loaded.
    private func saveActive() {
        guard var q = active, let s = current else { return }
        let pos = Int(position * 1000)
        q.currentSong = s.id
        q.positionMs = pos
        app.store.setCurrent(q)
        saveResume(s, pos)
    }

    /// The song's bytes, wherever they are now: resolved when the song is
    /// opened, so a download that finished meanwhile is used.
    private func bytes(for song: Int) -> Bytes? {
        guard let s = app.catalog.byId[song] else { return nil }
        return app.cache.open(s)
    }

    // MARK: Play order

    /// Shuffled, a queue keeps its play order across edits: songs keep
    /// their places, new ones go in at random after the playing one.
    /// Shuffling afresh puts the playing song first, as Media3 does.
    private func rebuildOrder(keepingShuffle: Bool, shuffle: Bool? = nil) {
        let cur = current?.id
        if !(shuffle ?? self.shuffle) {
            order = items
        } else if keepingShuffle && !order.isEmpty {
            let present = Set(items)
            var kept = order.filter { present.contains($0) }
            let have = Set(kept)
            let at = cur.flatMap { kept.firstIndex(of: $0) } ?? -1
            for id in items where !have.contains(id) {
                kept.insert(id, at: Int.random(in: (at + 1)...kept.count))
            }
            order = kept
        } else {
            var rest = items.filter { $0 != cur }.shuffled()
            if let cur, items.contains(cur) { rest.insert(cur, at: 0) }
            order = rest
        }
        publishUpcoming()
    }

    /// The index in `items` of the song after `items[i]` in play order.
    private func nextIndex(after i: Int?, repeat wrap: Bool) -> Int? {
        guard let i, items.indices.contains(i), let at = order.firstIndex(of: items[i]) else { return nil }
        let id = at + 1 < order.count ? order[at + 1] : wrap ? order.first : nil
        return id.flatMap { items.firstIndex(of: $0) }
    }

    private func previousIndex(before i: Int) -> Int? {
        guard let at = order.firstIndex(of: items[i]), at > 0 else { return nil }
        return items.firstIndex(of: order[at - 1])
    }

    /// What the engine's queue reads for the song after the one it is
    /// decoding: kept in step with the play order.
    private func publishUpcoming() {
        upcomingState.set(items: items, order: order, repeatMode: repeatMode)
    }

    func pauseAfter(_ song: Int?) { upcomingState.setPauseAfter(song) }

    // MARK: Following the engine

    private func tick() {
        guard current != nil, commands == 0 else { return }
        guard let p = engine.position() else {
            if isPlaying && !ended { reachedEnd() }
            return
        }
        if p.song != current?.id, let s = app.catalog.byId[p.song] {
            songChangedByItself(to: s)
        }
        position = p.seconds
        decodedDuration = p.duration
        waiting = isPlaying && app.cache.failing(p.song)
        guard isPlaying else { return }
        ticks += 1
        // Every 10 s of playing: save the open listen, and every 30 s the positions.
        if ticks % 40 == 0 { saveListen() }
        if ticks % 120 == 0, let q = active, let s = current {
            let pos = Int(position * 1000)
            var row = q
            row.currentSong = s.id
            row.positionMs = pos
            app.store.setCurrent(row)
            saveResume(s, pos)
            report(q, s, pos, true)
        }
    }

    /// The engine went on to the next song by itself: the last one finished.
    private func songChangedByItself(to s: Song) {
        if let finished = current {
            close("finished", toMs: finished.durationMs > 0 ? finished.durationMs : Int(position * 1000))
            // Heard to the end: a long file's resume point is done with.
            if isLong(finished) { app.store.resume(finished.id, nil) }
        }
        sleep.onNextSong()
        current = s
        failures = 0
        position = 0
        applyTempo()
        let start = startPosition(s)
        if let start { seek(to: Double(start) / 1000) }
        openListen(s, start ?? 0)
        afterSongChange(s, startMs: start ?? 0)
        app.cache.poke()
        media.update()
    }

    /// Everything scheduled has played: the end of the queue, or a sleep timer's song.
    private func reachedEnd() {
        let s = current
        close("finished", toMs: s.map { $0.durationMs > 0 ? $0.durationMs : Int(position * 1000) } ?? 0)
        if let s, isLong(s) { app.store.resume(s.id, nil) }
        let bySleep = upcomingState.pauseAfterSong != nil && sleep.mode != nil
        setPlaying(false)
        if bySleep, let s, let i = items.firstIndex(of: s.id),
            let n = nextIndex(after: i, repeat: repeatMode == "queue"),
            let next = app.catalog.byId[items[n]]
        {
            // Paused where the song ended; Play goes on with the next one.
            sleep.onPausedAtEnd()
            start(next, at: 0, paused: true)
            afterSongChange(next, startMs: 0)
            return
        }
        sleep.onPausedAtEnd()
        ended = true
    }

    /// A song that cannot be opened or read is passed over, as the phone does;
    /// a queue of them is not tried forever.
    private func songFailed(_ song: Int, _ error: Error) {
        failures += 1
        guard song == current?.id, failures < max(1, items.count), let i = currentIndex,
            let n = nextIndex(after: i, repeat: repeatMode == "queue")
        else { return }
        jump(to: n, end: "skipped")
    }

    /// Headphones out, or another output: pause, as the phone does when its
    /// audio becomes "noisy". The engine stopped; Play opens the song again.
    private func outputChanged() {
        needsRestart = true
        if isPlaying {
            setPlaying(false)
        }
    }

    private func setPlaying(_ playing: Bool) {
        let changed = playing != isPlaying
        isPlaying = playing
        if playing { failures = 0 }
        if let o = open {
            if playing {
                started(o)
            } else if o.since > 0 {
                count(o)
                o.since = 0
            }
        }
        guard changed, let q = active, let s = current else { return }
        media.update()
        let pos = Int(position * 1000)
        report(q, s, pos, playing)
        if !playing {
            var row = q
            row.currentSong = s.id
            row.positionMs = pos
            app.store.setCurrent(row)
            saveResume(s, pos)
        }
        app.cache.poke()
    }

    // MARK: Long files (plan 009)

    private func isLong(_ s: Song) -> Bool {
        s.durationMs >= app.setting("longFiles.minMinutes", 15) * 60_000
    }

    /// Where to start `s`: its resume point under "auto"; under "ask", offered instead.
    private func startPosition(_ s: Song) -> Int? {
        offerResume = nil
        guard isLong(s), let r = app.db.resume(s.id), !r.deleted, r.positionMs > 0 else { return nil }
        switch app.setting("longFiles.resume", "auto") {
        case "auto": return r.positionMs
        case "ask":
            offerResume = (s, r.positionMs)
            return nil
        default: return nil
        }
    }

    private func saveResume(_ s: Song, _ pos: Int) {
        if isLong(s) && pos > 0 { app.store.resume(s.id, pos) }
    }

    // MARK: Listens (plan 008)

    final class Open: Codable {
        let song: Int
        /// When it began to play: a song loaded but never started is not a listen yet.
        var startedAt = 0
        let fromMs: Int
        let queue: String
        let source: String
        let shuffle: Bool
        var heardMs = 0
        var lastPos = 0
        var lastAt = 0
        /// Not saved: when the current stretch of playing began.
        var since: Double = 0

        init(song: Int, fromMs: Int, queue: String, source: String, shuffle: Bool) {
            self.song = song
            self.fromMs = fromMs
            self.queue = queue
            self.source = source
            self.shuffle = shuffle
            lastPos = fromMs
        }

        enum CodingKeys: String, CodingKey {
            case song, startedAt, fromMs, queue, source, shuffle, heardMs, lastPos, lastAt
        }
    }

    private func openListen(_ s: Song, _ from: Int) {
        let q = active
        let o = Open(
            song: s.id, fromMs: from, queue: q?.id ?? "",
            source: q.map { app.prefs.queueSource($0.id) } ?? "",
            shuffle: shuffle)
        open = o
        if isPlaying { started(o) }
    }

    private func started(_ o: Open) {
        o.since = ProcessInfo.processInfo.systemUptime
        if o.startedAt == 0 { o.startedAt = nowMs() }
    }

    /// Adds the time played since the last count, as song time: two minutes
    /// at 1.5× heard three minutes of the song, which is what the play
    /// count's "half the song heard" compares (plan 008).
    private func count(_ o: Open) {
        guard o.since > 0 else { return }
        let now = ProcessInfo.processInfo.systemUptime
        o.heardMs += Int((now - o.since) * 1000 * heardSpeed)
        o.since = now
    }

    private func close(_ end: String, toMs: Int? = nil) {
        guard let o = open else { return }
        open = nil
        app.prefs.openListen = ""
        count(o)
        // A song that never actually played was not a listen.
        guard o.heardMs > 0 else { return }
        app.store.play(listen(o, end, toMs ?? Int(position * 1000), nowMs()))
    }

    private func listen(_ o: Open, _ end: String, _ toMs: Int, _ endedAt: Int) -> Listen {
        let start = Date(timeIntervalSince1970: Double(o.startedAt) / 1000)
        return Listen(
            song: o.song, startedAt: o.startedAt, endedAt: endedAt, ms: o.heardMs, fromMs: o.fromMs,
            toMs: toMs,
            end: end, source: o.source, queue: o.queue, shuffle: o.shuffle,
            utcOffset: TimeZone.current.secondsFromGMT(for: start) / 60)
    }

    private func saveListen() {
        guard let o = open else { return }
        count(o)
        o.lastPos = Int(position * 1000)
        o.lastAt = nowMs()
        if let data = try? JSONEncoder().encode(o) {
            app.prefs.openListen = String(decoding: data, as: UTF8.self)
        }
    }

    /// A listen still open when the app last quit (or crashed) is closed as interrupted.
    private func closeInterrupted() {
        let text = app.prefs.openListen
        guard !text.isEmpty else { return }
        app.prefs.openListen = ""
        guard let o = try? JSONDecoder().decode(Open.self, from: Data(text.utf8)), o.heardMs > 0 else {
            return
        }
        app.store.play(listen(o, "interrupted", o.lastPos, o.lastAt))
    }

    /// On quitting: the open listen and the positions are saved, as on pause.
    public func quitting() {
        if isPlaying { pause() }
        saveListen()
    }

    // MARK: Helpers

    public static func orderOf(_ queues: [QueueRow], _ order: JSON?) -> [QueueRow] {
        let ids = order?.array?.compactMap(\.string) ?? []
        let byId = Dictionary(queues.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        var seen = Set<String>()
        let named = ids.filter { seen.insert($0).inserted }.compactMap { byId[$0] }
        let namedIds = Set(named.map(\.id))
        let rest = queues.filter { !namedIds.contains($0.id) }.sorted { $0.usedAt < $1.usedAt }
        return named + rest
    }
}

private func distinct(_ ids: [Int]) -> [Int] {
    var seen = Set<Int>()
    return ids.filter { seen.insert($0).inserted }
}

/// The play order as the engine's queue reads it, to pick the song after the
/// one it has decoded to the end.
final class Upcoming: @unchecked Sendable {
    private let lock = NSLock()
    private var items: [Int] = []
    private var order: [Int] = []
    private var repeatMode = "off"
    private var pauseAfter: Int?

    var pauseAfterSong: Int? { lock.withLock { pauseAfter } }

    func set(items: [Int], order: [Int], repeatMode: String) {
        lock.withLock {
            self.items = items
            self.order = order
            self.repeatMode = repeatMode
        }
    }

    func setPauseAfter(_ song: Int?) { lock.withLock { pauseAfter = song } }

    func next(after song: Int) -> Int? {
        lock.withLock {
            if pauseAfter == song { return nil }
            if repeatMode == "song" { return song }
            guard let at = order.firstIndex(of: song) else { return nil }
            if at + 1 < order.count { return order[at + 1] }
            return repeatMode == "queue" ? order.first : nil
        }
    }
}
