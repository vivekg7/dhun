import AVFoundation
import AppKit
import DhunKit
import SwiftUI

/// The menu bar control (plan 024): the song, the controls, the seek bar and
/// the queue picker, while the window is closed.
struct MenuBarPlayer: View {
    @Environment(AppModel.self) private var app
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        let p = app.playback
        VStack(spacing: 12) {
            HStack(spacing: 12) {
                Cover(song: p.current, size: 64)
                VStack(alignment: .leading, spacing: 3) {
                    Text(p.current?.title ?? "Nothing playing").fontWeight(.semibold).lineLimit(2)
                    Text(p.current?.displayArtist ?? "").font(.caption).foregroundStyle(.secondary).lineLimit(
                        1)
                    if let q = p.active {
                        Text(q.name).font(.caption).foregroundStyle(.tertiary).lineLimit(1)
                    }
                }
                Spacer(minLength: 0)
            }
            // Hand-off here too: with the window closed, this is where the Mac is used.
            if let np = p.handoff, let s = app.catalog.byId[np.song] {
                Button {
                    p.continueFrom(np)
                } label: {
                    Label(
                        "Continue from \(np.deviceName.isEmpty ? "another device" : np.deviceName): \(s.title)",
                        systemImage: "arrow.triangle.2.circlepath"
                    )
                    .lineLimit(1)
                    .frame(maxWidth: .infinity)
                }
                .buttonStyle(.bordered)
            }
            Transport()
            Seeker()
            HStack {
                // Truncated, not at its full width: a long queue name pushed the buttons out of the panel.
                Menu {
                    ForEach(Array(p.ordered.enumerated()), id: \.element.id) { i, q in
                        Button("\(i + 1). \(q.name)") { p.switchTo(q.id) }
                    }
                } label: {
                    Text("Queue: \(p.active?.name ?? "none")").lineLimit(1).truncationMode(.tail)
                }
                .menuStyle(.borderlessButton)
                .frame(maxWidth: .infinity, alignment: .leading)
                FavoriteButton().buttonStyle(.borderless)
                Button("Mini Player", systemImage: "pip") { openWindow(id: "mini") }
                    .labelStyle(.iconOnly).buttonStyle(.borderless).help("Mini player")
                Button("Open Dhun", systemImage: "macwindow") {
                    openWindow(id: "main")
                    NSApp.activate()
                }
                .labelStyle(.iconOnly).buttonStyle(.borderless).help("Open the Dhun window")
            }
            .font(.caption)
        }
        .padding(14)
        .frame(width: 320)
    }
}

/// The floating mini window (plan 024), the counterpart of the phone's
/// pill: small, above other windows, where it was last left. A plain
/// window, since a title bar's buttons took a strip as tall as the player:
/// it is dragged by anywhere on it, and a close button shows over the cover
/// while the pointer is on it. It never becomes the key window, so ⌘W does
/// not reach it.
struct MiniPlayer: View {
    @Environment(AppModel.self) private var app
    @Environment(\.dismissWindow) private var dismissWindow
    @State private var hovering = false

    var body: some View {
        let p = app.playback
        HStack(spacing: 10) {
            Cover(song: p.current, size: 52)
                .overlay(alignment: .topLeading) {
                    if hovering {
                        Button("Close", systemImage: "xmark.circle.fill") { dismissWindow(id: "mini") }
                            .labelStyle(.iconOnly).buttonStyle(.plain)
                            .font(.title3).symbolRenderingMode(.palette)
                            .foregroundStyle(.white, .black.opacity(0.6))
                            .padding(2)
                            .transition(.opacity)
                    }
                }
            VStack(alignment: .leading, spacing: 4) {
                Text(p.current?.title ?? "Nothing playing").font(.callout.weight(.medium)).lineLimit(1)
                Text(p.current?.displayArtist ?? "").font(.caption).foregroundStyle(.secondary).lineLimit(1)
                Transport(compact: true)
            }
            .frame(width: 190, alignment: .leading)
        }
        .padding(10)
        .background(.background, in: .rect(cornerRadius: 14))
        .gesture(WindowDragGesture())
        .onHover { h in withAnimation(.easeOut(duration: 0.15)) { hovering = h } }
    }
}

/// A file opened from Finder (plan 021): plays in a small window of its own,
/// through a second engine. It makes no queue and logs no listen; it stops
/// when the window closes.
@MainActor
final class FilePlayerWindow: NSObject, NSWindowDelegate {
    private let window: NSWindow
    private let model: FileModel
    private let closed: (FilePlayerWindow) -> Void

    init(url: URL, app: AppModel, closed: @escaping (FilePlayerWindow) -> Void) {
        model = FileModel(url: url)
        self.closed = closed
        window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 380, height: 150), styleMask: [.titled, .closable],
            backing: .buffered, defer: false)
        super.init()
        window.title = url.lastPathComponent
        window.isReleasedWhenClosed = false
        window.contentView = NSHostingView(rootView: FilePlayerView(model: model).themed(app))
        window.delegate = self
        window.center()
    }

    func show() {
        window.makeKeyAndOrderFront(nil)
        NSApp.activate()
        model.start()
    }

    func close() { window.close() }

    func windowWillClose(_ notification: Notification) {
        model.stop()
        closed(self)
    }
}

@MainActor @Observable
final class FileModel {
    let url: URL
    var title: String
    var artist = ""
    var art: NSImage?
    var playing = false
    var position = 0.0
    var duration = 0.0
    var error: String?
    @ObservationIgnored private let engine = Engine()
    @ObservationIgnored private let queue = DispatchQueue(label: "dhun.file")
    @ObservationIgnored private var timer: Timer?
    /// Where to open the file again after the output changed (headphones out), which stops the engine.
    @ObservationIgnored private var restartAt: Double?

    init(url: URL) {
        self.url = url
        title = url.deletingPathExtension().lastPathComponent
    }

    func start() {
        Task { await readTags() }
        engine.interrupted = { [weak self] in
            Task { @MainActor in
                guard let self else { return }
                self.restartAt = self.position
                self.playing = false
            }
        }
        queue.async { [engine, url] in
            do {
                try engine.play(song: 0, bytes: try FileBytes(url))
                DispatchQueue.main.async { MainActor.assumeIsolated { self.playing = true } }
            } catch {
                DispatchQueue.main.async { MainActor.assumeIsolated { self.error = "Cannot play this file" } }
            }
        }
        timer = Timer.scheduledTimer(withTimeInterval: 0.25, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated {
                guard let self, self.restartAt == nil else { return }
                if let p = self.engine.position() {
                    self.position = p.seconds
                    self.duration = p.duration
                } else if self.playing {
                    self.playing = false
                }
            }
        }
    }

    func toggle() {
        if playing {
            engine.pause()
            playing = false
        } else if let at = restartAt {
            restartAt = nil
            playing = true
            queue.async { [engine, url] in try? engine.play(song: 0, bytes: try FileBytes(url), from: at) }
        } else if engine.position() == nil {
            seek(0)
            playing = true
        } else {
            try? engine.resume()
            playing = true
        }
    }

    func seek(_ s: Double) {
        let wasPlaying = playing || engine.position() == nil
        queue.async { [engine, url] in
            if engine.position() == nil {
                try? engine.play(song: 0, bytes: try FileBytes(url), from: s, paused: !wasPlaying)
            } else {
                try? engine.seek(to: s)
            }
        }
    }

    func stop() {
        timer?.invalidate()
        queue.async { [engine] in engine.stop() }
    }

    private func readTags() async {
        let asset = AVURLAsset(url: url)
        guard let items = try? await asset.load(.commonMetadata) else { return }
        for i in items {
            switch i.commonKey {
            case .commonKeyTitle: if let v = try? await i.load(.stringValue), !v.isEmpty { title = v }
            case .commonKeyArtist: if let v = try? await i.load(.stringValue) { artist = v }
            case .commonKeyArtwork: if let d = try? await i.load(.dataValue) { art = NSImage(data: d) }
            default: break
            }
        }
    }
}

struct FilePlayerView: View {
    let model: FileModel
    @State private var dragging: Double?

    var body: some View {
        HStack(spacing: 14) {
            Group {
                if let art = model.art {
                    Image(nsImage: art).resizable().aspectRatio(contentMode: .fill)
                } else {
                    Placeholder()
                }
            }
            .frame(width: 96, height: 96)
            .clipShape(RoundedRectangle(cornerRadius: 6))
            VStack(alignment: .leading, spacing: 6) {
                Text(model.title).font(.headline).lineLimit(2)
                if !model.artist.isEmpty { Text(model.artist).font(.caption).foregroundStyle(.secondary) }
                if let e = model.error { Text(e).foregroundStyle(.red).font(.caption) }
                HStack {
                    Button {
                        model.toggle()
                    } label: {
                        Image(systemName: model.playing ? "pause.circle.fill" : "play.circle.fill").font(
                            .system(size: 28)
                        )
                        .foregroundStyle(.tint)
                    }
                    .buttonStyle(.borderless)
                    .keyboardShortcut(.space, modifiers: [])
                    Slider(
                        value: Binding(get: { dragging ?? model.position }, set: { dragging = $0 }),
                        in: 0...max(model.duration, 1)
                    ) { editing in
                        if !editing, let d = dragging {
                            model.seek(d)
                            dragging = nil
                        }
                    }
                    .controlSize(.small)
                }
                Text("\(clock(model.position)) / \(clock(model.duration))").font(.caption).monospacedDigit()
                    .foregroundStyle(.secondary)
            }
        }
        .padding(16)
        .frame(width: 380)
    }
}

/// The Controls menu: the transport and its keyboard shortcuts.
struct Commands: SwiftUI.Commands {
    let app: AppModel
    let nav: Nav

    var body: some SwiftUI.Commands {
        CommandMenu("Controls") {
            Button(app.playback.isPlaying ? "Pause" : "Play") { app.playback.toggle() }
                .keyboardShortcut(.space, modifiers: [])
            Button("Next") { app.playback.next() }.keyboardShortcut(.rightArrow, modifiers: .command)
            Button("Previous") { app.playback.previous() }.keyboardShortcut(.leftArrow, modifiers: .command)
            Divider()
            Button("Shuffle") { app.playback.toggleShuffle() }.keyboardShortcut(
                "s", modifiers: [.command, .option])
            Button("Repeat") { app.playback.cycleRepeat() }.keyboardShortcut(
                "r", modifiers: [.command, .option])
            Divider()
            Button("Go to Current Song") { if let s = app.playback.current { nav.goToAlbum(s, app) } }
                .keyboardShortcut("l", modifiers: .command)
            Button("Show Queue") {
                nav.inspectorTab = .queue
                nav.inspector = true
            }
            .keyboardShortcut("u", modifiers: [.command, .option])
            Button("Show Lyrics") {
                nav.inspectorTab = .lyrics
                nav.inspector = true
            }
            .keyboardShortcut("l", modifiers: [.command, .option])
        }
        CommandGroup(after: .textEditing) {
            Button("Find") {
                nav.findRequests += 1
            }
            .keyboardShortcut("f", modifiers: .command)
        }
        CommandGroup(after: .newItem) {
            Button("Sync Now") { Task { await app.sync.now() } }.keyboardShortcut("r", modifiers: .command)
        }
    }
}
