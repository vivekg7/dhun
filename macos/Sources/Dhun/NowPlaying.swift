import DhunKit
import SwiftUI

/// Now playing, along the bottom of the window: the song, the controls and
/// the seek bar, then favourite, speed, sleep, lyrics and the queue.
struct NowPlayingBar: View {
    @Environment(AppModel.self) private var app
    @Environment(Nav.self) private var nav

    var body: some View {
        let p = app.playback
        let forward = direction.forward(to: p.current?.id, in: p.items)
        HStack(spacing: 14) {
            ZStack(alignment: .leading) {
                HStack(spacing: 10) {
                    Cover(song: p.current, size: 46)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(p.current?.title ?? "Nothing playing").fontWeight(.medium).lineLimit(1)
                        // Redrawn on a clock too: "Sleep in 12 min" counts down on its own.
                        TimelineView(.periodic(from: .now, by: 15)) { _ in
                            Text(subtitle).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                        }
                    }
                }
                .frame(width: 250, alignment: .leading)
                // The next song comes in from the right, the previous from the left, as on the
                // phone. The song leaving only fades, quickly: it keeps the transition it was
                // last drawn with, so a slide of its own would go the way the change before went.
                .id(p.current?.id)
                .transition(
                    .asymmetric(
                        insertion: .move(edge: forward ? .trailing : .leading).combined(with: .opacity),
                        removal: .opacity.animation(.easeOut(duration: 0.12))))
            }
            .frame(width: 250, alignment: .leading)
            .clipped()
            .animation(.smooth(duration: 0.3), value: p.current?.id)
            .contentShape(Rectangle())
            .onTapGesture(count: 2) { if let s = p.current { nav.goToAlbum(s, app) } }
            .contextMenu { if let s = p.current { SongMenu(songs: [s]) } }
            // The song on the left, the extras on the right, however wide the window.
            Spacer(minLength: 0)

            VStack(spacing: 4) {
                Transport()
                Seeker()
            }
            .frame(maxWidth: 560)
            Spacer(minLength: 0)

            HStack(spacing: 4) {
                FavoriteButton()
                LaterButton()
                SpeedButton()
                SleepButton()
                Button {
                    let showing = nav.inspector && nav.inspectorTab == .lyrics
                    nav.inspectorTab = .lyrics
                    nav.inspector = !showing
                } label: {
                    Image(systemName: "quote.bubble")
                }
                .help("Lyrics")
                .accessibilityLabel("Lyrics")
                Button {
                    let showing = nav.inspector && nav.inspectorTab == .queue
                    nav.inspectorTab = .queue
                    nav.inspector = !showing
                } label: {
                    Image(systemName: "list.bullet")
                }
                .help("Playing queue")
                .accessibilityLabel("Playing queue")
            }
            .buttonStyle(.borderless)
            .frame(width: 230, alignment: .trailing)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 9)
        .background(.bar)
        .overlay(alignment: .top) { Divider() }
    }

    @State private var direction = Direction()

    private var subtitle: String {
        let p = app.playback
        guard let s = p.current else { return "" }
        if p.waiting { return "Waiting for the network…" }
        var parts = [s.displayArtist]
        if let q = p.active { parts.append(q.name) }
        if let sleep = p.sleep.label { parts.append("Sleep \(sleep)") }
        return parts.joined(separator: " · ")
    }
}

/// Which way the queue moved when the song changed: on to a later song, or
/// back. Worked out while drawing, so the slide that change starts already
/// goes the right way; off the end of a repeating queue to its start is
/// still forward.
final class Direction {
    private var last: (id: Int?, at: Int?) = (nil, nil)
    private var wasForward = true

    func forward(to id: Int?, in items: [Int]) -> Bool {
        guard id != last.id else { return wasForward }
        let at = id.flatMap { items.firstIndex(of: $0) }
        if let from = last.at, let to = at {
            wasForward = to > from || (from == items.count - 1 && to == 0)
        }
        last = (id, at)
        return wasForward
    }
}

struct Transport: View {
    @Environment(AppModel.self) private var app
    var compact = false

    var body: some View {
        let p = app.playback
        HStack(spacing: compact ? 14 : 20) {
            if !compact {
                Button {
                    p.toggleShuffle()
                } label: {
                    Image(systemName: "shuffle").foregroundStyle(
                        p.shuffle ? AnyShapeStyle(.tint) : AnyShapeStyle(.secondary))
                }
                .help(p.shuffle ? "Shuffle is on" : "Shuffle")
                .accessibilityLabel("Shuffle")
                .accessibilityValue(p.shuffle ? "On" : "Off")
            }
            Button {
                p.previous()
            } label: {
                Image(systemName: "backward.fill")
            }.help("Previous").accessibilityLabel("Previous")
            Button {
                p.toggle()
            } label: {
                Image(systemName: p.isPlaying ? "pause.circle.fill" : "play.circle.fill").font(
                    .system(size: compact ? 26 : 32)
                )
                .foregroundStyle(.tint)
                .contentTransition(.symbolEffect(.replace))
            }
            .help(p.isPlaying ? "Pause" : "Play")
            .accessibilityLabel(p.isPlaying ? "Pause" : "Play")
            Button {
                p.next()
            } label: {
                Image(systemName: "forward.fill")
            }.help("Next").accessibilityLabel("Next")
            if !compact {
                Button {
                    p.cycleRepeat()
                } label: {
                    Image(systemName: p.repeatMode == "song" ? "repeat.1" : "repeat")
                        .foregroundStyle(
                            p.repeatMode == "off" ? AnyShapeStyle(.secondary) : AnyShapeStyle(.tint)
                        )
                        .contentTransition(.symbolEffect(.replace))
                }
                .help(
                    ["off": "Repeat", "queue": "Repeating the queue", "song": "Repeating this song"][
                        p.repeatMode] ?? ""
                )
                .accessibilityLabel("Repeat")
                .accessibilityValue(
                    ["off": "Off", "queue": "The queue", "song": "This song"][p.repeatMode] ?? "")
            }
        }
        .buttonStyle(.borderless)
        .disabled(p.current == nil)
    }
}

/// The seek bar: dragging shows where it will land, and seeks on release.
struct Seeker: View {
    @Environment(AppModel.self) private var app
    @State private var dragging: Double?

    var body: some View {
        let p = app.playback
        let length = max(p.duration, 1)
        HStack(spacing: 8) {
            Text(clock(dragging ?? p.position)).font(.caption).monospacedDigit().foregroundStyle(.secondary)
                .frame(width: 48, alignment: .trailing)
            Slider(
                value: Binding(get: { min(dragging ?? p.position, length) }, set: { dragging = $0 }),
                in: 0...length
            ) { editing in
                if !editing, let d = dragging {
                    p.seek(to: d)
                    dragging = nil
                }
            }
            .controlSize(.small)
            Text(clock(p.duration)).font(.caption).monospacedDigit().foregroundStyle(.secondary)
                .frame(width: 48, alignment: .leading)
        }
        .disabled(p.current == nil)
    }
}

struct FavoriteButton: View {
    @Environment(AppModel.self) private var app
    var body: some View {
        let s = app.playback.current
        let on = s.map { id in app.favorites.contains { $0.song == id.id } } ?? false
        Button {
            if let s { app.store.mark(Store.fav, s.id, !on) }
        } label: {
            Image(systemName: on ? "heart.fill" : "heart").foregroundStyle(
                on ? AnyShapeStyle(.tint) : AnyShapeStyle(.primary)
            )
            .toggled(on, of: s?.id)
        }
        .help(on ? "Remove from Favorites" : "Add to Favorites")
        .accessibilityLabel(on ? "Remove from Favorites" : "Add to Favorites")
        .disabled(s == nil)
    }
}

extension View {
    /// A mark turned on fills and bounces, as on the phone; turned off it
    /// only empties. A song shown that has it on already does not bounce.
    func toggled(_ on: Bool, of song: Int?) -> some View {
        modifier(Toggled(on: on, song: song))
    }
}

private struct Toggled: ViewModifier {
    let on: Bool
    let song: Int?
    @State private var bounces = 0

    private struct Mark: Equatable {
        let on: Bool
        let song: Int?
    }

    func body(content: Content) -> some View {
        content
            .contentTransition(.symbolEffect(.replace))
            .symbolEffect(.bounce, value: bounces)
            .onChange(of: Mark(on: on, song: song)) { was, now in
                if now.on, !was.on, now.song == was.song { bounces += 1 }
            }
    }
}

/// Listen Later for the song playing, as the phone's Now playing has.
struct LaterButton: View {
    @Environment(AppModel.self) private var app
    var body: some View {
        let s = app.playback.current
        let on = s.map { id in app.listenLater.contains { $0.song == id.id } } ?? false
        Button {
            if let s { app.store.mark(Store.later, s.id, !on) }
        } label: {
            Image(systemName: on ? "clock.fill" : "clock").foregroundStyle(
                on ? AnyShapeStyle(.tint) : AnyShapeStyle(.primary)
            )
            .toggled(on, of: s?.id)
        }
        .help(on ? "Remove from Listen Later" : "Listen Later")
        .accessibilityLabel(on ? "Remove from Listen Later" : "Listen Later")
        .disabled(s == nil)
    }
}

/// Speed and pitch (plan 016), for the song playing or as this Mac's everyday one.
struct SpeedButton: View {
    @Environment(AppModel.self) private var app
    @State private var open = false

    var body: some View {
        Button {
            open = true
        } label: {
            if let l = app.playback.tempo.label {
                Text(l).font(.caption.weight(.semibold)).foregroundStyle(.tint)
            } else {
                Image(systemName: "gauge.with.dots.needle.33percent")
            }
        }
        .help("Speed and pitch")
        .accessibilityLabel("Speed and pitch")
        .popover(isPresented: $open, arrowEdge: .top) { SpeedPanel().padding(16).frame(width: 300) }
    }
}

struct SpeedPanel: View {
    @Environment(AppModel.self) private var app
    @State private var onlyThisSong = false
    /// The speed under the finger: applied, and synced, once on release, as on the phone.
    @State private var dragging: Double?

    var body: some View {
        let p = app.playback
        let t = p.tempo
        VStack(alignment: .leading, spacing: 12) {
            Text("Speed \(Tempo.format(dragging ?? t.speed))×").font(.headline)
            Slider(
                value: Binding(get: { dragging ?? t.speed }, set: { dragging = ($0 * 20).rounded() / 20 }),
                in: Tempo.minSpeed...Tempo.maxSpeed
            ) { editing in
                if !editing, let d = dragging {
                    set(Tempo(speed: d, semitones: t.semitones))
                    dragging = nil
                }
            }
            HStack {
                ForEach([0.75, 1, 1.25, 1.5, 2], id: \.self) { v in
                    Button("\(Tempo.format(v))×") { set(Tempo(speed: v, semitones: t.semitones)) }
                        .buttonStyle(.bordered).controlSize(.small)
                }
            }
            Stepper(
                "Pitch \(t.semitones > 0 ? "+" : t.semitones < 0 ? "−" : "")\(abs(t.semitones)) semitones",
                value: Binding(get: { t.semitones }, set: { set(Tempo(speed: t.speed, semitones: $0)) }),
                in: -Tempo.maxSemitones...Tempo.maxSemitones)
            // A binding, not onChange: setting the toggle on appear must not write the setting again.
            Toggle(
                "Only for this song",
                isOn: Binding(
                    get: { onlyThisSong },
                    set: { on in
                        onlyThisSong = on
                        if on { p.setTempo(t, onlyThisSong: true) } else { p.clearSongTempo() }
                    })
            )
            .disabled(p.current == nil)
            Text(
                onlyThisSong
                    ? "This song keeps its own, on all your devices."
                    : "For every song on this Mac without its own."
            )
            .font(.caption).foregroundStyle(.secondary)
            Button("Reset") { set(.normal) }.disabled(t.isNormal)
        }
        .onAppear { onlyThisSong = p.songTempo != nil }
    }

    private func set(_ t: Tempo) { app.playback.setTempo(t, onlyThisSong: onlyThisSong) }
}

/// The sleep timer (plan 014).
struct SleepButton: View {
    @Environment(AppModel.self) private var app
    @State private var open = false

    var body: some View {
        Button {
            open = true
        } label: {
            Image(systemName: app.playback.sleep.mode == nil ? "moon.zzz" : "moon.zzz.fill")
                .foregroundStyle(
                    app.playback.sleep.mode == nil ? AnyShapeStyle(.primary) : AnyShapeStyle(.tint)
                )
                .contentTransition(.symbolEffect(.replace))
        }
        .help(app.playback.sleep.label.map { "Sleep \($0)" } ?? "Sleep timer")
        .accessibilityLabel(app.playback.sleep.label.map { "Sleep \($0)" } ?? "Sleep timer")
        .popover(isPresented: $open, arrowEdge: .top) {
            SleepPanel { open = false }.padding(16).frame(width: 260)
        }
    }
}

struct SleepPanel: View {
    @Environment(AppModel.self) private var app
    let done: () -> Void
    @State private var minutes = 20
    @State private var songs = 3

    var body: some View {
        let t = app.playback.sleep
        VStack(alignment: .leading, spacing: 10) {
            Text(t.label.map { "Stops \($0)" } ?? "Sleep timer").font(.headline)
            HStack {
                ForEach([15, 30, 45, 60], id: \.self) { m in
                    Button("\(m)m") {
                        t.minutes(m); done()
                    }.buttonStyle(.bordered).controlSize(.small)
                }
            }
            HStack {
                Stepper("\(minutes) minutes", value: $minutes, in: 1...600)
                Button("Start") {
                    t.minutes(minutes); done()
                }
            }
            Divider()
            Button("At the end of this song") {
                t.endOfSong(); done()
            }
            HStack {
                Stepper("After \(songs) songs", value: $songs, in: 2...100)
                Button("Start") {
                    t.songs(songs); done()
                }
            }
            Button("At the end of the queue") {
                t.endOfQueue(); done()
            }
            if t.mode != nil {
                Divider()
                Button("Turn Off", role: .destructive) {
                    t.cancel(); done()
                }
            }
        }
        .buttonStyle(.borderless)
        .disabled(app.playback.current == nil)
    }
}

/// The right-hand panel: the playing queue or the lyrics.
struct Inspector: View {
    @Environment(AppModel.self) private var app
    @Environment(Nav.self) private var nav

    var body: some View {
        @Bindable var nav = nav
        VStack(spacing: 0) {
            Picker("", selection: $nav.inspectorTab) {
                Text("Queue").tag(Nav.InspectorTab.queue)
                Text("Lyrics").tag(Nav.InspectorTab.lyrics)
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .padding(10)
            Group {
                switch nav.inspectorTab {
                case .queue:
                    if let q = app.playback.active {
                        QueueSongs(queue: q)
                    } else {
                        ContentUnavailableView("Nothing playing", systemImage: "list.bullet")
                    }
                case .lyrics: LyricsView()
                }
            }
            .transition(.opacity)
            .animation(.easeInOut(duration: 0.15), value: nav.inspectorTab)
        }
    }
}

/// Lyrics (plan 015): synced lines follow the song and a click on one plays
/// from it; plain lyrics are just text.
struct LyricsView: View {
    @Environment(AppModel.self) private var app
    @State private var state = Lyrics.State.loading
    /// A scroll by hand holds the lines where they are for a few seconds, as on the phone.
    @State private var heldUntil = Date.distantPast
    /// While lyrics are shown and playing the display stays awake, as the phone's screen does.
    @State private var awake: NSObjectProtocol?

    var body: some View {
        let p = app.playback
        Group {
            switch state {
            case .loading: ProgressView()
            case .none: ContentUnavailableView("No lyrics", systemImage: "quote.bubble")
            case .offline: ContentUnavailableView("Lyrics need the server", systemImage: "wifi.slash")
            case .shown(let l):
                let at = l.at(Int(p.position * 1000))
                ScrollViewReader { proxy in
                    ScrollView {
                        VStack(alignment: .leading, spacing: 10) {
                            ForEach(Array(l.lines.enumerated()), id: \.offset) { i, line in
                                Text(line.text.isEmpty ? " " : line.text)
                                    .font(l.synced ? .title3.weight(i == at ? .semibold : .regular) : .body)
                                    .foregroundStyle(
                                        !l.synced || i == at
                                            ? AnyShapeStyle(.primary) : AnyShapeStyle(.secondary)
                                    )
                                    .id(i)
                                    .onTapGesture { if l.synced { p.seek(to: Double(line.ms) / 1000) } }
                            }
                        }
                        .padding(16)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    .onScrollPhaseChange { _, phase in
                        if phase != .idle && phase != .animating { heldUntil = Date().addingTimeInterval(4) }
                    }
                    .onChange(of: at) { _, i in
                        guard Date() >= heldUntil else { return }
                        withAnimation { proxy.scrollTo(max(0, i), anchor: .center) }
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .task(id: p.current?.id) {
            guard let s = p.current else { return state = .none }
            await app.lyrics.load(s) { state = $0 }
        }
        .onChange(of: p.isPlaying, initial: true) { _, playing in keepAwake(playing) }
        .onDisappear { keepAwake(false) }
    }

    private func keepAwake(_ on: Bool) {
        if on, awake == nil {
            awake = ProcessInfo.processInfo.beginActivity(
                options: .idleDisplaySleepDisabled, reason: "Showing lyrics")
        } else if !on, let a = awake {
            ProcessInfo.processInfo.endActivity(a)
            awake = nil
        }
    }
}
