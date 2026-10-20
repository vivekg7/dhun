// swift-tools-version: 6.0
// The macOS app: docs/plans/024_macos_app.md. A package rather than an
// .xcodeproj so it builds, tests and reviews as plain text.
import PackageDescription

let package = Package(
    name: "Dhun",
    platforms: [.macOS("26.0")],
    targets: [
        // Everything testable without a window: playback, data, sync.
        .target(name: "DhunKit"),
        // The app: windows, menus and the menu bar control.
        .executableTarget(name: "Dhun", dependencies: ["DhunKit"]),
        .testTarget(name: "DhunKitTests", dependencies: ["DhunKit"], resources: [.copy("Fixtures")]),
    ]
)
