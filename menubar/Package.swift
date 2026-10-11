// swift-tools-version: 6.0
import PackageDescription

// Atlas's menu bar app. It shows what the local viewer reports and runs
// `atlas` commands; it holds no state or policy of its own.
let package = Package(
    name: "AtlasBar",
    platforms: [.macOS(.v14)],
    products: [.executable(name: "AtlasBar", targets: ["AtlasBar"])],
    targets: [
        .target(name: "AtlasKit"),
        .target(name: "AtlasUI", dependencies: ["AtlasKit"]),
        .executableTarget(name: "AtlasBar", dependencies: ["AtlasKit", "AtlasUI"]),
        .testTarget(name: "AtlasKitTests", dependencies: ["AtlasKit"]),
    ],
    swiftLanguageModes: [.v5]
)
