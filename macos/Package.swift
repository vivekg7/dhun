// swift-tools-version: 6.0
// The macOS app: docs/plans/024_macos_app.md. A package rather than an
// .xcodeproj so it builds, tests and reviews as plain text.
import PackageDescription

let package = Package(
    name: "Dhun",
    platforms: [.macOS(.v14)],
    targets: [
        // Everything testable without a window: playback, data, sync.
        .target(name: "DhunKit"),
        // The playback spike's harness (plan 024, step 0).
        // A top-level script, so Swift 5 mode spares it strict concurrency.
        .executableTarget(
            name: "dhun-play", dependencies: ["DhunKit"],
            swiftSettings: [.swiftLanguageMode(.v5)]),
        .testTarget(name: "DhunKitTests", dependencies: ["DhunKit"], resources: [.copy("Fixtures")]),
    ]
)
