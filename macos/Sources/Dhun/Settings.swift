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

/// Settings (⌘,), the phone's pages in Mac form. A setting is a short title
/// and its value; what it does is on the How it works tab, which an ⓘ opens
/// at the right place (plan 034).
struct SettingsView: View {
    enum Tab: Hashable { case appearance, playback, downloads, account, guide }
    @State private var nav = SettingsNav()

    var body: some View {
        @Bindable var nav = nav
        TabView(selection: $nav.tab) {
            AppearanceSettings().tabItem { Label("Appearance", systemImage: "paintpalette") }.tag(
                Tab.appearance)
            PlaybackSettings().tabItem { Label("Playback", systemImage: "play.circle") }.tag(Tab.playback)
            StorageSettings().tabItem { Label("Downloads", systemImage: "arrow.down.circle") }.tag(
                Tab.downloads)
            AccountSettings().tabItem { Label("Account", systemImage: "person.crop.circle") }.tag(Tab.account)
            GuideView(at: $nav.guideAt).tabItem { Label("How it works", systemImage: "questionmark.circle") }
                .tag(Tab.guide)
        }
        .environment(nav)
        .frame(width: 520, height: 440)
    }
}

/// The open tab, and the `###` heading How it works shows. An object, not a
/// closure, in the environment: a closure is new on every redraw, and would
/// redraw every ⓘ with it.
@MainActor @Observable final class SettingsNav {
    var tab = SettingsView.Tab.appearance
    var guideAt: String?
}

/// The ⓘ beside a setting whose guide section says more than its title.
struct InfoButton: View {
    let section: String
    @Environment(SettingsNav.self) private var nav

    var body: some View {
        Button {
            nav.guideAt = section
            nav.tab = .guide
        } label: {
            Image(systemName: "info.circle")
        }
        .buttonStyle(.borderless)
        .help("How it works")
    }
}

/// A value's label, with " (default)" on the default: a menu's items take no style of their own.
func marked(_ label: String, _ isDefault: Bool) -> String { isDefault ? label + " (default)" : label }

/// A form's last row: Reset to Defaults, after asking.
struct ResetButton: View {
    let title: String
    let message: String
    let reset: () -> Void

    var body: some View {
        HStack {
            Button("Reset to Defaults…") {
                if Prompt.confirm(title, message, action: "Reset") { reset() }
            }
            Spacer()
        }
        .padding([.horizontal, .bottom], 20)
    }
}

struct AppearanceSettings: View {
    @Environment(AppModel.self) private var app

    var body: some View {
        @Bindable var app = app
        VStack(spacing: 0) {
            Form {
                SwiftUI.Section("Appearance") {
                    Picker("Theme", selection: $app.appearance.theme) {
                        ForEach(
                            [("system", "Follow the system"), ("light", "Light"), ("dark", "Dark")], id: \.0
                        ) {
                            Text(marked($0.1, $0.0 == Prefs.Default.theme)).tag($0.0)
                        }
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
                                    .help(marked(p.name, i == Prefs.Default.palette))
                            }
                        }
                    }
                }
            }
            .formStyle(.grouped)
            ResetButton(
                title: "Reset Appearance settings?", message: "The theme and accent colour of this Mac.",
                reset: app.resetAppearance)
        }
    }
}

/// Synced settings: they follow the user to every device (plans 009, 010).
struct PlaybackSettings: View {
    @Environment(AppModel.self) private var app

    var body: some View {
        VStack(spacing: 0) {
            Text("These follow you to all your devices.").font(.callout).foregroundStyle(.secondary)
                .padding(.top, 16)
            Form {
                SwiftUI.Section("Buttons") {
                    choice("Previous", Synced.previousRestart) { $0 ? "Restart song first" : "Previous song" }
                }
                SwiftUI.Section("Long files") {
                    choice("Remember position", Synced.minMinutes, info: "Remember position") {
                        "\($0) min and longer"
                    }
                    .help("For audiobooks, podcasts and mixes.")
                    choice("When played again", Synced.resume) {
                        switch $0 {
                        case "auto": "Resume"
                        case "ask": "Ask"
                        default: "Start over"
                        }
                    }
                    choice("Previous button", Synced.previous, info: "Previous button") {
                        switch $0 {
                        case "keep": "Previous file"
                        case "seek": "Seek back (headset)"
                        case "ignore": "Ignore headset"
                        default: "Same as songs"
                        }
                    }
                    .help("On the screen, Previous always goes to the previous file.")
                    choice("Next button", Synced.next, info: "Next button") {
                        $0 == "seek" ? "Seek forward (headset)" : "Next file"
                    }
                    choice("Seek back", Synced.seekBack) { "\($0) s" }
                    choice("Seek forward", Synced.seekForward) { "\($0) s" }
                    choice("Rewind on resume", Synced.rewind) { $0 == 0 ? "Off" : "\($0) s" }
                    choice("After pauses of", Synced.rewindAfter) { "\($0) min" }
                        .disabled(app.setting(Synced.rewind) == 0)
                    choice("Cache ahead", Synced.cacheAhead, info: "Song cache") {
                        $0 == 0 ? "Whole file" : "\($0) min"
                    }
                    .help("A long file is cached around where you are, not whole.")
                }
                SwiftUI.Section("Listen Later") {
                    Toggle("Remove when finished", isOn: synced(Synced.autoRemove))
                    choice("Finished at", Synced.finishedPercent) { "\($0)%" }
                        .disabled(!app.setting(Synced.autoRemove))
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
            ResetButton(
                title: "Reset Playback settings?", message: "They follow you, so all your devices change.",
                reset: app.store.resetPlayback)
        }
    }

    /// A synced setting's menu of values, the default marked, maybe with an ⓘ.
    private func choice<V>(
        _ title: String, _ key: Synced.Key<V>, info: String? = nil, label: @escaping (V) -> String
    ) -> some View {
        Picker(selection: synced(key)) {
            ForEach(key.options, id: \.self) { Text(marked(label($0), $0 == key.def)).tag($0) }
        } label: {
            HStack(spacing: 4) {
                Text(title)
                if let info { InfoButton(section: info) }
            }
        }
    }

    private func synced<V>(_ key: Synced.Key<V>) -> Binding<V> {
        Binding(get: { app.setting(key) }, set: { app.store.setting(key.name, $0.json) })
    }
}

/// This Mac's storage and network, so not synced (plans 012, 019).
struct StorageSettings: View {
    @Environment(AppModel.self) private var app

    var body: some View {
        @Bindable var app = app
        VStack(spacing: 0) {
            Form {
                SwiftUI.Section {
                    LabeledContent("Used", value: formatBytes(app.downloads.status.usedBytes))
                    Picker("Storage limit", selection: $app.storage.downloadLimitGb) {
                        ForEach([2, 5, 10, 20, 50, 0], id: \.self) {
                            Text(
                                marked($0 == 0 ? "No limit" : "\($0) GB", $0 == Prefs.Default.downloadLimitGb)
                            ).tag($0)
                        }
                    }
                    Toggle("Download on a phone's hotspot", isOn: $app.storage.downloadOnExpensive)
                } header: {
                    Text("Downloads")
                } footer: {
                    Text("At the limit, downloads stop. Nothing is deleted.").font(.caption)
                        .foregroundStyle(.secondary)
                }
                SwiftUI.Section {
                    LabeledContent("Used", value: formatBytes(app.cache.used))
                    Picker(selection: $app.storage.cacheLimitGb) {
                        ForEach([1, 3, 5, 10, 0], id: \.self) {
                            Text(marked($0 == 0 ? "Off" : "\($0) GB", $0 == Prefs.Default.cacheLimitGb)).tag(
                                $0)
                        }
                    } label: {
                        HStack(spacing: 4) {
                            Text("Storage limit")
                            InfoButton(section: "Song cache")
                        }
                    }
                } header: {
                    Text("Song cache")
                }
            }
            .formStyle(.grouped)
            ResetButton(
                title: "Reset Downloads settings?", message: "Nothing downloaded is deleted.",
                reset: app.resetStorage)
        }
    }
}

/// How it works (docs/GUIDE.md, copied into `guideText` by scripts/guide.sh):
/// headings, paragraphs, list items and bold, which is all the guide uses.
struct GuideView: View {
    @Binding var at: String?

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                VStack(alignment: .leading, spacing: 8) {
                    ForEach(Array(guideBlocks.enumerated()), id: \.offset) { _, b in
                        block(b).id(b.text)
                    }
                }
                .padding(20)
                .frame(maxWidth: .infinity, alignment: .leading)
                .textSelection(.enabled)
            }
            .task(id: at) {
                guard let at else { return }
                // After the tab's first layout, or there is nothing to scroll to yet.
                try? await Task.sleep(for: .milliseconds(50))
                withAnimation { proxy.scrollTo(at, anchor: .top) }
            }
        }
    }

    @ViewBuilder private func block(_ b: GuideBlock) -> some View {
        let text = Text((try? AttributedString(markdown: b.text)) ?? AttributedString(b.text))
        switch b.kind {
        case .heading(1): text.font(.title2.weight(.semibold))
        case .heading(2): text.font(.title3.weight(.semibold)).padding(.top, 10)
        case .heading: text.font(.headline).padding(.top, 4)
        case .item:
            HStack(alignment: .firstTextBaseline, spacing: 6) {
                Text("•"); text
            }
        case .paragraph: text
        }
    }
}

struct GuideBlock {
    enum Kind: Equatable {
        case heading(Int), paragraph, item
    }
    let kind: Kind
    let text: String
}

/// The guide's blocks: a heading per `#` line, a list item per `- ` line,
/// and the lines between blank lines joined into a paragraph.
let guideBlocks: [GuideBlock] = {
    var blocks: [GuideBlock] = []
    var open = false  // the last block takes the next line
    for line in guideText.split(separator: "\n", omittingEmptySubsequences: false) {
        let l = line.trimmingCharacters(in: .whitespaces)
        if l.isEmpty {
            open = false
        } else if l.hasPrefix("#") {
            let level = l.prefix { $0 == "#" }.count
            blocks.append(
                GuideBlock(
                    kind: .heading(level),
                    text: String(l.dropFirst(level)).trimmingCharacters(in: .whitespaces)))
            open = false
        } else if l.hasPrefix("- ") {
            blocks.append(GuideBlock(kind: .item, text: String(l.dropFirst(2))))
            open = true
        } else if open, let last = blocks.popLast() {
            blocks.append(GuideBlock(kind: last.kind, text: last.text + " " + l))
        } else {
            blocks.append(GuideBlock(kind: .paragraph, text: l))
            open = true
        }
    }
    return blocks
}()

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
