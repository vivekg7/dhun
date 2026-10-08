import AppKit
import DhunKit
import SwiftUI

/// The app (plan 024): one library window with a sidebar, a menu bar
/// control, a floating mini player, Settings, and a small player for files
/// opened from Finder. Closing the window does not stop the music; ⌘Q does.
@main
struct DhunApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var delegate
    @State private var nav = Nav()

    var body: some Scene {
        Window("Dhun", id: "main") {
            Root()
                .environment(delegate.app)
                .environment(nav)
                .frame(minWidth: 820, minHeight: 520)
                .themed(delegate.app)
        }
        .defaultSize(width: 1180, height: 760)
        .commands { Commands(app: delegate.app, nav: nav) }

        Window("Mini Player", id: "mini") {
            MiniPlayer()
                .environment(delegate.app)
                .environment(nav)
                .themed(delegate.app)
        }
        .windowResizability(.contentSize)
        .windowStyle(.hiddenTitleBar)
        .windowLevel(.floating)
        .windowBackgroundDragBehavior(.enabled)
        .defaultPosition(.topTrailing)

        Settings {
            SettingsView()
                .environment(delegate.app)
                .themed(delegate.app)
        }

        MenuBarExtra {
            MenuBarPlayer()
                .environment(delegate.app)
                .environment(nav)
                .themed(delegate.app)
        } label: {
            Image(systemName: delegate.app.playback.isPlaying ? "music.note" : "music.note.list")
        }
        .menuBarExtraStyle(.window)
    }
}

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    let app: AppModel = {
        let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        do {
            return try AppModel(files: support.appendingPathComponent("Dhun"))
        } catch {
            fatalError("Cannot open Dhun's data folder: \(error)")
        }
    }()
    private var filePlayers: [FilePlayerWindow] = []

    func applicationDidFinishLaunching(_ n: Notification) {
        let ws = NSWorkspace.shared.notificationCenter
        ws.addObserver(forName: NSWorkspace.didWakeNotification, object: nil, queue: .main) { [app] _ in
            MainActor.assumeIsolated { app.refresh() }
        }
        app.refresh()
        watchKeys()
    }

    /// Space plays or pauses and ⌘← ⌘→ skip wherever the focus is, as in
    /// Music: a list or the sidebar would otherwise take Space for itself.
    /// While typing they stay the text's own keys, and a file opened from
    /// Finder keeps Space for its own player.
    private func watchKeys() {
        NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [app] e in
            let handled = MainActor.assumeIsolated { () -> Bool in
                guard let w = NSApp.keyWindow, w.attachedSheet == nil, NSApp.modalWindow == nil,
                    !(w.delegate is FilePlayerWindow)
                else { return false }
                let typing = w.firstResponder is NSText
                let mods = e.modifierFlags.intersection(.deviceIndependentFlagsMask)
                    .subtracting([.numericPad, .function])
                switch (e.keyCode, mods) {
                case (49, []) where !typing:
                    app.playback.toggle()
                    return true
                case (123, .command) where typing, (124, .command) where typing:
                    // Before the Controls menu sees them: the start or end of the line.
                    w.firstResponder?.keyDown(with: e)
                    return true
                default:
                    return false
                }
            }
            return handled ? nil : e
        }
    }

    func applicationDidBecomeActive(_ n: Notification) { app.refresh() }

    /// The music goes on with the window closed, from the menu bar.
    func applicationShouldTerminateAfterLastWindowClosed(_ s: NSApplication) -> Bool { false }

    func applicationWillTerminate(_ n: Notification) { app.playback.quitting() }

    /// Open With from Finder (plan 021): a small window of its own; no queue,
    /// no listen logged. One file at a time, as on the phone: a new one
    /// replaces the one playing rather than playing over it.
    func application(_ application: NSApplication, open urls: [URL]) {
        guard let url = urls.first else { return }
        app.playback.pause()
        for w in filePlayers { w.close() }
        let w = FilePlayerWindow(url: url, app: app) { [weak self] closed in
            self?.filePlayers.removeAll { $0 === closed }
        }
        filePlayers.append(w)
        w.show()
    }
}
