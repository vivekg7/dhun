import DhunKit
import SwiftUI

/// The search filters (plan 041), in a menu beside the toolbar's search
/// field rather than in the page: the phone's chips in Mac form. Its icon
/// fills while anything is applied, and the results' subtitle names it.
/// Picking one shows the songs it finds, with the field empty or not.
struct FilterMenu: View {
    @Environment(Nav.self) private var nav

    var body: some View {
        let f = nav.filters
        let applied = !f.isEmpty || f.sort != .best
        let decades = stride(
            from: Calendar.current.component(.year, from: .now) / 10 * 10, through: 1950, by: -10
        )
        .map { $0...$0 + 9 }
        let presets = decades + [1...1949]
        Menu {
            Menu(f.years.map { "Year: \(SearchFilters.yearsLabel($0))" } ?? "Year") {
                Picker("Year", selection: binding(\.years)) {
                    Text("Any Year").tag(ClosedRange<Int>?.none)
                    ForEach(presets, id: \.self) { Text(SearchFilters.yearsLabel($0)).tag(Optional($0)) }
                    // A range typed in, ticked as the one applied.
                    if let y = f.years, !presets.contains(y) {
                        Text(SearchFilters.yearsLabel(y)).tag(Optional(y))
                    }
                }
                .pickerStyle(.inline)
                .labelsHidden()
                Divider()
                Button("Range…") { nav.sheet = .years }
            }
            Button(SearchFilters.names(f.artists).map { "Artist: \($0)…" } ?? "Artist…") {
                nav.sheet = .artists
            }
            Button(SearchFilters.names(f.genres).map { "Genre: \($0)…" } ?? "Genre…") { nav.sheet = .genres }
            Toggle("Popular", isOn: binding(\.popular))
            Picker("Sort", selection: binding(\.sort)) {
                ForEach(SearchFilters.Sort.allCases, id: \.self) { Text($0.rawValue).tag($0) }
            }
            Divider()
            Button("Clear Filters") { nav.filters = SearchFilters() }.disabled(!applied)
        } label: {
            Label(
                "Filter",
                systemImage: applied
                    ? "line.3.horizontal.decrease.circle.fill" : "line.3.horizontal.decrease.circle")
        }
        .help(
            applied
                ? "Filtered: " + dotted([f.describe(), f.sort == .best ? "" : f.sort.rawValue])
                : "Filter search")
    }

    private func binding<V>(_ at: WritableKeyPath<SearchFilters, V>) -> Binding<V> {
        Binding(
            get: { nav.filters[keyPath: at] },
            set: {
                var f = nav.filters
                f[keyPath: at] = $0
                nav.filter(f)
            })
    }
}

/// Several artists or genres, picked from the library's list and searched
/// as the window searches them. The ones picked stay at the top, then the
/// most songs first: the names people look for are the big ones.
struct PickSheet: View {
    @Environment(AppModel.self) private var app
    @Environment(Nav.self) private var nav
    @Environment(\.dismiss) private var dismiss
    let genres: Bool
    @State private var chosen: Set<String>?
    @State private var text = ""

    var body: some View {
        // Built when first opened, not with the window.
        let index = genres ? app.catalog.genreIndex : app.catalog.artistIndex
        let picked = genres ? nav.filters.genres : nav.filters.artists
        let chosen = self.chosen ?? picked
        let lowered = Set(picked.map { $0.lowercased() })
        let first = index.items.filter { lowered.contains($0.id) }
        let rest = index.filter(SearchQuery(text)).filter { !lowered.contains($0.id) }.sorted {
            $0.songs.count > $1.songs.count
        }
        VStack(alignment: .leading, spacing: 10) {
            Text(genres ? "Genres" : "Artists").font(.headline)
            TextField("Search", text: $text).textFieldStyle(.roundedBorder)
            List(first + rest) { g in
                let on = chosen.contains { $0.lowercased() == g.id }
                Toggle(
                    isOn: Binding(
                        get: { on },
                        set: { _ in
                            self.chosen =
                                on ? chosen.filter { $0.lowercased() != g.id } : chosen.union([g.name])
                        })
                ) {
                    HStack {
                        Text(g.name).lineLimit(1)
                        Spacer()
                        Text("\(g.songs.count)").foregroundStyle(.secondary).monospacedDigit()
                    }
                }
            }
            .listStyle(.bordered)
            HStack {
                Button("Clear") { apply([]) }
                Spacer()
                Button("Cancel", role: .cancel) { dismiss() }.keyboardShortcut(.cancelAction)
                Button("Apply") { apply(chosen) }.keyboardShortcut(.defaultAction)
            }
        }
        .padding(16)
        .frame(width: 400, height: 520)
    }

    private func apply(_ names: Set<String>) {
        var f = nav.filters
        if genres { f.genres = names } else { f.artists = names }
        nav.filter(f)
        dismiss()
    }
}

/// Any range of years typed in; the decades are a click away in the menu.
struct YearsSheet: View {
    @Environment(Nav.self) private var nav
    @Environment(\.dismiss) private var dismiss
    @State private var from = ""
    @State private var to = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Years").font(.headline)
            HStack {
                TextField("From", text: digits($from)).frame(width: 80)
                Text("–")
                TextField("To", text: digits($to)).frame(width: 80)
            }
            .textFieldStyle(.roundedBorder)
            HStack {
                Button("Any Year") { apply(nil) }
                Spacer()
                Button("Cancel", role: .cancel) { dismiss() }.keyboardShortcut(.cancelAction)
                Button("Apply") {
                    let now = Calendar.current.component(.year, from: .now)
                    let a = Int(from)
                    let b = Int(to)
                    // An open end runs to the first year, or to this one.
                    apply(a == nil && b == nil ? nil : min(a ?? 1, b ?? now)...max(a ?? 1, b ?? now))
                }
                .keyboardShortcut(.defaultAction)
            }
        }
        .padding(16)
        .frame(width: 280)
        .onAppear {
            from = nav.filters.years.map { String($0.lowerBound) } ?? ""
            to = nav.filters.years.map { String($0.upperBound) } ?? ""
        }
    }

    private func digits(_ s: Binding<String>) -> Binding<String> {
        Binding(get: { s.wrappedValue }, set: { s.wrappedValue = String($0.filter(\.isNumber).prefix(4)) })
    }

    private func apply(_ years: ClosedRange<Int>?) {
        var f = nav.filters
        f.years = years
        nav.filter(f)
        dismiss()
    }
}
