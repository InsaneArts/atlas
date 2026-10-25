// swift-tools-version: 6.2
import PackageDescription

/// Swift 6 language mode makes strict concurrency checking complete; the
/// default isolation is nonisolated so nothing becomes MainActor by
/// accident. The two upcoming features are Approachable Concurrency: a
/// nonisolated async function runs on its caller's actor unless marked
/// `@concurrent`, and a conformance of an isolated type is inferred to share
/// its isolation.
let strict: [SwiftSetting] = [
  .defaultIsolation(nil),
  .enableUpcomingFeature("NonisolatedNonsendingByDefault"),
  .enableUpcomingFeature("InferIsolatedConformances"),
]

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
    .target(name: "AtlasControl", dependencies: ["AtlasService"], swiftSettings: strict),
    .target(
      name: "AtlasControlTesting",
      dependencies: ["AtlasControl", "AtlasService"],
      swiftSettings: strict
    ),
    .testTarget(
      name: "AtlasControlTests",
      dependencies: [
        "AtlasControl", "AtlasControlTesting", "AtlasService",
        .product(name: "AsyncSupportTesting", package: "AsyncSupport"),
      ],
      swiftSettings: strict
    ),
  ],
  swiftLanguageModes: [.v6]
)
