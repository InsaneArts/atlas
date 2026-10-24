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

// The viewer service's model, its seam and its live client. Atlas.app's
// executable package depends on it by path.
//
// Level 0. Foundation only.
let package = Package(
  name: "AtlasService",
  platforms: [.macOS(.v14)],
  products: [
    .library(name: "AtlasService", targets: ["AtlasService"]),
    .library(name: "AtlasServiceTesting", targets: ["AtlasServiceTesting"]),
  ],
  targets: [
    .target(name: "AtlasService", swiftSettings: strict),
    .target(name: "AtlasServiceTesting", dependencies: ["AtlasService"], swiftSettings: strict),
    .testTarget(
      name: "AtlasServiceTests",
      dependencies: ["AtlasService", "AtlasServiceTesting"],
      swiftSettings: strict
    ),
  ],
  swiftLanguageModes: [.v6]
)
