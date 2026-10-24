// swift-tools-version: 6.2
import PackageDescription

// What a status means, as kinds the panel, the viewer and the setup window
// share. No words: the app words them.
//
// Level 1. Depends on AtlasService. Foundation only.
let package = Package(
  name: "AtlasStatus",
  platforms: [.macOS(.v14)],
  products: [
    .library(name: "AtlasStatus", targets: ["AtlasStatus"])
  ],
  dependencies: [
    .package(path: "../AtlasService")
  ],
  targets: [
    .target(name: "AtlasStatus", dependencies: ["AtlasService"]),
    .testTarget(
      name: "AtlasStatusTests",
      dependencies: [
        "AtlasStatus", "AtlasService",
        .product(name: "AtlasServiceTesting", package: "AtlasService"),
      ]
    ),
  ],
  swiftLanguageModes: [.v6]
)
