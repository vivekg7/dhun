import DhunKit
import SwiftUI

struct Sidebar: View {
    @Environment(AppModel.self) private var app
    @Environment(Nav.self) private var nav
    @State private var naming = false

    var body: some View {
        @Bindable var nav = nav
        List(selection: Binding(get: { nav.section }, set: { if let s = $0 { nav.open(s) } })) {
            Label("Queues", systemImage: "list.number").tag(Section.queues)
            SwiftUI.Section("Library") {
                Label("Folders", systemImage: "folder").tag(Section.folders)
                Label("Albums", systemImage: "square.stack").tag(Section.albums)
                Label("Artists", systemImage: "music.mic").tag(Section.artists)
                Label("Genres", systemImage: "guitars").tag(Section.genres)
            }
            SwiftUI.Section("Lists") {
                ForEach(ListKind.allCases, id: \.self) { k in
                    Label(k.title, systemImage: k.icon).tag(Section.list(k))
                        .dropDestination(for: String.self) { items, _ in
                            guard let mark = k.mark else { return false }
                            for id in songIds(items) { app.store.mark(mark, id, true) }
                            return true
                        }
                }
            }
            SwiftUI.Section("Playlists") {
                ForEach(app.playlists) { p in
                    Label(p.name, systemImage: p.shared ? "music.note.house" : "music.note.list")
                        .tag(Section.playlist(p.id))
                        .dropDestination(for: String.self) { items, _ in
                            guard editable(p) else { return false }
                            app.store.addToPlaylist(p, songIds(items))
                            return true
                        }
                }
            }
            Label("Downloads", systemImage: "arrow.down.circle").tag(Section.downloads)
        }
        .listStyle(.sidebar)
        .safeAreaInset(edge: .bottom) {
            HStack {
                Button("New Playlist", systemImage: "plus") { naming = true }
                    .labelStyle(.titleAndIcon)
                    .buttonStyle(.borderless)
                Spacer()
                SyncStatus()
            }
            .padding(10)
        }
        .sheet(isPresented: $naming) {
            NameSheet(title: "New playlist", action: "Create") { name in
                let p = app.store.createPlaylist(name, [])
                nav.open(.playlist(p.id))
            }
        }
    }

    private func editable(_ p: Playlist) -> Bool { !p.shared || app.admin }
}

/// Songs dragged between views travel as their ids.
func songIds(_ items: [String]) -> [Int] { items.flatMap { $0.split(separator: ",").compactMap { Int($0) } } }

/// Offline, syncing, or an error, small at the foot of the sidebar.
struct SyncStatus: View {
    @Environment(AppModel.self) private var app

    var body: some View {
        if !app.reachable {
            Label("Offline", systemImage: "wifi.slash").font(.caption).foregroundStyle(.secondary)
                .help(
                    "The server cannot be reached. Downloaded and cached songs still play; changes are kept and sent later."
                )
        } else if app.syncing {
            ProgressView().controlSize(.small)
        } else if let e = app.syncError {
            Image(systemName: "exclamationmark.triangle").foregroundStyle(.orange).help(e)
        }
    }
}

/// Asks for a name: a new playlist or queue, or a rename.
struct NameSheet: View {
    let title: String
    let action: String
    var initial = ""
    let done: (String) -> Void
    @State private var name = ""
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text(title).font(.headline)
            TextField("Name", text: $name).frame(width: 300).onSubmit(submit)
            HStack {
                Spacer()
                Button("Cancel", role: .cancel) { dismiss() }.keyboardShortcut(.cancelAction)
                Button(action, action: submit).keyboardShortcut(.defaultAction)
                    .disabled(name.trimmingCharacters(in: .whitespaces).isEmpty)
            }
        }
        .padding(20)
        .onAppear { name = initial }
    }

    private func submit() {
        let n = name.trimmingCharacters(in: .whitespaces)
        guard !n.isEmpty else { return }
        done(n)
        dismiss()
    }
}
