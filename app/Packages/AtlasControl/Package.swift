// swift-tools-version: 6.2
import PackageDescription

// Mapping the person's mouse, keys and text to the viewer service's input
// actions, and the queue that sends them.
//
// Level 1. Depends on AtlasService. Foundation only.
let package = Package(
  name: "AtlasControl",
  platforms: [.macOS(.v14)],
  products: [
    .library(name: "AtlasControl", targets: ["AtlasControl"]),
    .library(name: "AtlasControlTesting", targets: ["AtlasControlTesting"]),
  ],
  dependencies: [
    // AsyncSupport is here only for AsyncSupportTesting, whose Ceiling the
    // tests wait through; AtlasControl itself depends on AtlasService alone.
    .package(path: "../AsyncSupport"),
    .package(path: "../AtlasService"),
  ],
  targets: [
    .target(name: "AtlasControl", dependencies: ["AtlasService"]),
    .target(name: "AtlasControlTesting", dependencies: ["AtlasControl", "AtlasService"]),
    .testTarget(
      name: "AtlasControlTests",
      dependencies: [
        "AtlasControl", "AtlasControlTesting", "AtlasService",
        .product(name: "AsyncSupportTesting", package: "AsyncSupport"),
      ]
    ),
  ],
  swiftLanguageModes: [.v6]
)
