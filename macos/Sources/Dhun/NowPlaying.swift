import DhunKit
import SwiftUI

/// Now playing, along the bottom of the window: the song, the controls and
/// the seek bar, then favourite, speed, sleep, lyrics and the queue.
struct NowPlayingBar: View {
    @Environment(AppModel.self) private var app
    @Environment(Nav.self) private var nav

    var body: some View {
        let p = app.playback
        HStack(spacing: 14) {
            HStack(spacing: 10) {
                Cover(song: p.current, size: 46)
                VStack(alignment: .leading, spacing: 2) {
                    Text(p.current?.title ?? "Nothing playing").fontWeight(.medium).lineLimit(1)
                    Text(subtitle).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                }
            }
            .frame(width: 250, alignment: .leading)
            .contentShape(Rectangle())
            .onTapGesture(count: 2) { if let s = p.current { nav.goToAlbum(s, app) } }
            .contextMenu { if let s = p.current { SongMenu(songs: [s]) } }

            VStack(spacing: 4) {
                Transport()
                Seeker()
            }
            .frame(maxWidth: 560)

            HStack(spacing: 4) {
                FavoriteButton()
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
                Button {
                    let showing = nav.inspector && nav.inspectorTab == .queue
                    nav.inspectorTab = .queue
                    nav.inspector = !showing
                } label: {
                    Image(systemName: "list.bullet")
                }
                .help("Playing queue")
            }
            .buttonStyle(.borderless)
            .frame(width: 200, alignment: .trailing)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 9)
        .background(.bar)
        .overlay(alignment: .top) { Divider() }
    }

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
            }
            Button {
                p.previous()
            } label: {
                Image(systemName: "backward.fill")
            }.help("Previous")
            Button {
                p.toggle()
            } label: {
                Image(systemName: p.isPlaying ? "pause.circle.fill" : "play.circle.fill").font(
                    .system(size: compact ? 26 : 32)
                )
                .foregroundStyle(.tint)
            }
            .help(p.isPlaying ? "Pause" : "Play")
            Button {
                p.next()
            } label: {
                Image(systemName: "forward.fill")
            }.help("Next")
            if !compact {
                Button {
                    p.cycleRepeat()
                } label: {
                    Image(systemName: p.repeatMode == "song" ? "repeat.1" : "repeat")
                        .foregroundStyle(
                            p.repeatMode == "off" ? AnyShapeStyle(.secondary) : AnyShapeStyle(.tint))
                }
                .help(
                    ["off": "Repeat", "queue": "Repeating the queue", "song": "Repeating this song"][
                        p.repeatMode] ?? "")
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
                on ? AnyShapeStyle(.tint) : AnyShapeStyle(.primary))
        }
        .help(on ? "Remove from Favorites" : "Add to Favorites")
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
        .popover(isPresented: $open, arrowEdge: .top) { SpeedPanel().padding(16).frame(width: 300) }
    }
}

struct SpeedPanel: View {
    @Environment(AppModel.self) private var app
    @State private var onlyThisSong = false

    var body: some View {
        let p = app.playback
        let t = p.tempo
        VStack(alignment: .leading, spacing: 12) {
            Text("Speed \(Tempo.format(t.speed))×").font(.headline)
            Slider(
                value: Binding(
                    get: { t.speed },
                    set: { set(Tempo(speed: ($0 * 20).rounded() / 20, semitones: t.semitones)) }),
                in: Tempo.minSpeed...Tempo.maxSpeed)
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
            Toggle("Only for this song", isOn: $onlyThisSong)
                .disabled(p.current == nil)
                .onChange(of: onlyThisSong) { _, on in
                    if on { p.setTempo(t, onlyThisSong: true) } else { p.clearSongTempo() }
                }
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
                    app.playback.sleep.mode == nil ? AnyShapeStyle(.primary) : AnyShapeStyle(.tint))
        }
        .help(app.playback.sleep.label.map { "Sleep \($0)" } ?? "Sleep timer")
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
    }
}

/// Lyrics (plan 015): synced lines follow the song and a click on one plays
/// from it; plain lyrics are just text.
struct LyricsView: View {
    @Environment(AppModel.self) private var app
    @State private var state = Lyrics.State.loading

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
                    .onChange(of: at) { _, i in withAnimation { proxy.scrollTo(max(0, i), anchor: .center) } }
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .task(id: p.current?.id) {
            guard let s = p.current else { return state = .none }
            await app.lyrics.load(s) { state = $0 }
        }
    }
}
