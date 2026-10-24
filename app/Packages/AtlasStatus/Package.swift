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
    .target(name: "AtlasStatus", dependencies: ["AtlasService"], swiftSettings: strict),
    .testTarget(
      name: "AtlasStatusTests",
      dependencies: [
        "AtlasStatus", "AtlasService",
        .product(name: "AtlasServiceTesting", package: "AtlasService"),
      ],
      swiftSettings: strict
    ),
  ],
  swiftLanguageModes: [.v6]
)
