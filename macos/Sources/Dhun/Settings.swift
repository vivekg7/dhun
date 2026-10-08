import DhunKit
import SwiftUI

struct SignInView: View {
    @Environment(AppModel.self) private var app
    @State private var server = ""
    @State private var user = ""
    @State private var password = ""
    @State private var error: String?
    @State private var busy = false

    var body: some View {
        VStack(spacing: 18) {
            Image(systemName: "music.note.house").font(.system(size: 44)).foregroundStyle(.tint)
            Text("Sign in to Dhun").font(.title2.weight(.semibold))
            if app.revoked {
                Text("This Mac was signed out on the server. Sign in again; nothing you changed is lost.")
                    .multilineTextAlignment(.center).foregroundStyle(.secondary)
            }
            Form {
                TextField("Server", text: $server, prompt: Text("nas, or its Tailscale name"))
                TextField("Name", text: $user)
                SecureField("Password", text: $password)
            }
            .formStyle(.grouped)
            .scrollDisabled(true)
            .frame(width: 380, height: 150)
            .onSubmit(signIn)
            if let error { Text(error).foregroundStyle(.red) }
            Button(action: signIn) {
                if busy { ProgressView().controlSize(.small) } else { Text("Sign In").frame(width: 120) }
            }
            .buttonStyle(.borderedProminent)
            .keyboardShortcut(.defaultAction)
            .disabled(busy || server.isEmpty || user.isEmpty || password.isEmpty)
        }
        .padding(40)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .onAppear {
            server = app.prefs.server
            user = app.prefs.user
        }
    }

    private func signIn() {
        busy = true
        error = nil
        Task {
            do {
                try await app.signIn(server: server, user: user, password: password)
            } catch let e as ApiError {
                error = e.code == 401 ? "Wrong name or password." : e.message
            } catch {
                self.error = "Cannot reach \(serverURL(server)): \(error.localizedDescription)"
            }
            busy = false
        }
    }
}

/// Settings (⌘,), the phone's pages in Mac form.
struct SettingsView: View {
    var body: some View {
        TabView {
            AppearanceSettings().tabItem { Label("Appearance", systemImage: "paintpalette") }
            PlaybackSettings().tabItem { Label("Playback", systemImage: "play.circle") }
            StorageSettings().tabItem { Label("Downloads", systemImage: "arrow.down.circle") }
            AccountSettings().tabItem { Label("Account", systemImage: "person.crop.circle") }
        }
        .frame(width: 520, height: 440)
    }
}

struct AppearanceSettings: View {
    @Environment(AppModel.self) private var app

    var body: some View {
        @Bindable var app = app
        Form {
            Picker("Theme", selection: $app.appearance.theme) {
                Text("Follow the system").tag("system")
                Text("Light").tag("light")
                Text("Dark").tag("dark")
            }
            LabeledContent("Accent colour") {
                HStack(spacing: 8) {
                    ForEach(Array(Palette.all.enumerated()), id: \.offset) { i, p in
                        Circle().fill(p.color).frame(width: 22, height: 22)
                            .overlay {
                                if i == app.appearance.palette {
                                    Circle().stroke(.primary, lineWidth: 2).padding(-3)
                                }
                            }
                            .onTapGesture { app.appearance.palette = i }
                            .help(p.name)
                    }
                }
            }
        }
        .formStyle(.grouped)
    }
}

/// Synced settings: they follow the user to every device (plans 009, 010).
struct PlaybackSettings: View {
    @Environment(AppModel.self) private var app

    var body: some View {
        Form {
            SwiftUI.Section {
                Picker("Continue long files", selection: synced("longFiles.minMinutes", 15)) {
                    ForEach([5, 10, 15, 20, 30, 60], id: \.self) { Text("\($0) minutes and longer").tag($0) }
                }
                Picker("Playing one again", selection: synced("longFiles.resume", "auto")) {
                    Text("Continue where you left it").tag("auto")
                    Text("Ask").tag("ask")
                    Text("Start over").tag("off")
                }
            } header: {
                Text("Long files")
            } footer: {
                Text(
                    "Audiobooks, podcasts and mixes this long continue where you left them, on all your devices."
                )
                .font(.caption).foregroundStyle(.secondary)
            }
            SwiftUI.Section("Listen Later") {
                Toggle("Remove what you finish", isOn: synced("listenLater.autoRemove", true))
                Picker("Finished at", selection: synced("listenLater.finishedPercent", 90)) {
                    ForEach([80, 90, 95, 100], id: \.self) { Text("\($0)% heard").tag($0) }
                }
                .disabled(!app.setting("listenLater.autoRemove", true))
            }
            SwiftUI.Section("Speed and pitch") {
                LabeledContent("Everyday on this Mac") {
                    Text(app.playback.everyday.label ?? "Normal")
                    Button("Reset") { app.playback.setTempo(.normal, onlyThisSong: false) }
                        .disabled(app.playback.everyday.isNormal)
                }
            }
        }
        .formStyle(.grouped)
    }

    private func synced(_ name: String, _ def: Int) -> Binding<Int> {
        Binding(get: { app.setting(name, def) }, set: { app.store.setting(name, JSON($0)) })
    }
    private func synced(_ name: String, _ def: String) -> Binding<String> {
        Binding(get: { app.setting(name, def) }, set: { app.store.setting(name, JSON($0)) })
    }
    private func synced(_ name: String, _ def: Bool) -> Binding<Bool> {
        Binding(get: { app.setting(name, def) }, set: { app.store.setting(name, JSON($0)) })
    }
}

/// This Mac's storage and network, so not synced (plans 012, 019).
struct StorageSettings: View {
    @Environment(AppModel.self) private var app

    var body: some View {
        @Bindable var app = app
        Form {
            SwiftUI.Section {
                LabeledContent("Used", value: formatBytes(app.downloads.status.usedBytes))
                Picker("Storage limit", selection: $app.storage.downloadLimitGb) {
                    ForEach([2, 5, 10, 20, 50, 0], id: \.self) {
                        Text($0 == 0 ? "No limit" : "\($0) GB").tag($0)
                    }
                }
                Toggle("Download on a phone's hotspot", isOn: $app.storage.downloadOnExpensive)
            } header: {
                Text("Downloads")
            } footer: {
                Text("At the limit, downloads stop. Nothing is deleted to make room.").font(.caption)
                    .foregroundStyle(.secondary)
            }
            SwiftUI.Section {
                Picker("Song cache", selection: $app.storage.cacheLimitGb) {
                    ForEach([1, 3, 5, 10, 0], id: \.self) { Text($0 == 0 ? "Off" : "\($0) GB").tag($0) }
                }
                LabeledContent("Used", value: formatBytes(app.cache.used))
            } footer: {
                Text(
                    "Songs you play, and the next ones in the queue (10, or 2 on a phone's hotspot), are kept so they play without waiting. At the limit, the songs played longest ago make room."
                )
                .font(.caption).foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
    }
}

struct AccountSettings: View {
    @Environment(AppModel.self) private var app
    @State private var members = false

    var body: some View {
        Form {
            SwiftUI.Section("Account") {
                LabeledContent(
                    "Signed in as",
                    value: app.signedIn ? "\(app.user)\(app.admin ? " (admin)" : "")" : "Not signed in")
                LabeledContent("Server", value: app.prefs.server)
                if !app.prefs.serverVersion.isEmpty {
                    LabeledContent("Server version", value: app.prefs.serverVersion)
                }
                LabeledContent("Last sync") {
                    if let e = app.syncError {
                        Text(e).foregroundStyle(.red)
                    } else {
                        Text(app.syncing ? "Syncing…" : "Up to date")
                    }
                }
                HStack {
                    Button("Sync Now") { Task { await app.sync.now() } }.disabled(!app.signedIn)
                    if app.admin { Button("Family Members…") { members = true } }
                    Spacer()
                    Button("Sign Out…", role: .destructive) {
                        if Prompt.confirm(
                            "Sign out?", "This Mac forgets your queues, downloads and changes not yet sent.",
                            action: "Sign Out")
                        {
                            Task { await app.signOut() }
                        }
                    }
                    .disabled(!app.signedIn)
                }
            }
            SwiftUI.Section("About") {
                LabeledContent(
                    "Dhun for Mac",
                    value: Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "dev")
                Link("Source code (GPL-3.0)", destination: URL(string: "https://github.com/vivekg7/dhun")!)
            }
        }
        .formStyle(.grouped)
        .sheet(isPresented: $members) { FamilyMembers() }
    }
}

/// Family members, for the admin (plan 023): list, add, reset a forgotten password.
struct FamilyMembers: View {
    @Environment(AppModel.self) private var app
    @Environment(\.dismiss) private var dismiss
    @State private var list: [Member] = []
    @State private var error: String?
    @State private var editing: Member?
    @State private var adding = false

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Family members").font(.headline)
            List(list) { m in
                HStack {
                    VStack(alignment: .leading) {
                        Text(m.name + (m.admin ? " (admin)" : ""))
                        Text(m.admin ? "Its password is changed on the NAS" : seen(m)).font(.caption)
                            .foregroundStyle(.secondary)
                    }
                    Spacer()
                    if !m.admin { Button("Reset Password…") { editing = m } }
                }
            }
            .frame(height: 220)
            if let error { Text(error).foregroundStyle(.red) }
            HStack {
                Button("Add Member…") { adding = true }
                Spacer()
                Button("Done") { dismiss() }.keyboardShortcut(.defaultAction)
            }
        }
        .padding(20)
        .frame(width: 420)
        .task { await load() }
        .sheet(isPresented: $adding) {
            MemberForm(title: "Add a family member", askName: true) { name, pw in
                try await app.api.addMember(name: name, password: pw)
                await load()
            }
        }
        .sheet(item: $editing) { m in
            MemberForm(title: "New password for \(m.name)", askName: false) { _, pw in
                try await app.api.resetPassword(user: m.id, password: pw)
                await load()
            }
        }
    }

    /// "Signed in on 2 devices · seen 3 hours ago", as on the phone.
    private func seen(_ m: Member) -> String {
        guard m.devices > 0 else { return "No device signed in" }
        let at = parseTime(m.lastSeenAt)
        let on = "Signed in on \(m.devices) device\(m.devices == 1 ? "" : "s")"
        guard at > 0 else { return on }
        let date = Date(timeIntervalSince1970: Double(at) / 1000)
        let ago =
            -date.timeIntervalSinceNow < 60
            ? "just now" : RelativeDateTimeFormatter().localizedString(for: date, relativeTo: Date())
        return "\(on) · seen \(ago)"
    }

    private func load() async {
        do {
            list = try await app.api.members()
            error = nil
        } catch {
            self.error = (error as? ApiError)?.message ?? "Cannot reach the server"
        }
    }
}

/// A name (when adding) and a password. The server's rules for both come
/// back as its own error, rather than copied here where they could drift.
struct MemberForm: View {
    let title: String
    let askName: Bool
    let save: (String, String) async throws -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var name = ""
    @State private var password = ""
    @State private var error: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(title).font(.headline)
            if askName { TextField("Name", text: $name) }
            // Shown as typed: the admin reads it out to the member, and a typo
            // in a hidden password would lock them out.
            TextField("Password", text: $password)
            if let error { Text(error).foregroundStyle(.red).font(.caption) }
            HStack {
                Spacer()
                Button("Cancel", role: .cancel) { dismiss() }
                Button("Save") {
                    Task {
                        do {
                            try await save(name.trimmingCharacters(in: .whitespaces), password)
                            dismiss()
                        } catch {
                            self.error = (error as? ApiError)?.message ?? error.localizedDescription
                        }
                    }
                }
                .keyboardShortcut(.defaultAction)
                .disabled(password.isEmpty || (askName && name.trimmingCharacters(in: .whitespaces).isEmpty))
            }
        }
        .padding(20)
        .frame(width: 340)
    }
}
