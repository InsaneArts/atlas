// swift-tools-version: 6.0
import PackageDescription

// Atlas.app's face: the menu bar, the viewer and the setup window. It shows
// what Atlas's local viewer service reports and runs `atlas` commands; the
// rules that keep the helper safe live in the `atlas` command line.
let package = Package(
    name: "Atlas",
    platforms: [.macOS(.v14)],
    products: [.executable(name: "AtlasApp", targets: ["AtlasApp"])],
    targets: [
        // Models, the service client and every decision: Foundation only.
        .target(name: "AtlasKit"),
        // Windows and views: AppKit and SwiftUI.
        .target(name: "AtlasUI", dependencies: ["AtlasKit"]),
        // The executable shipped beside `atlas` in Atlas.app.
        .executableTarget(name: "AtlasApp", dependencies: ["AtlasKit", "AtlasUI"]),
        .testTarget(name: "AtlasKitTests", dependencies: ["AtlasKit"]),
        .testTarget(name: "AtlasUITests", dependencies: ["AtlasKit", "AtlasUI"]),
    ],
    swiftLanguageModes: [.v6]
)
