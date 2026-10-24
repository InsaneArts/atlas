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

// The design system, components, each window's state and events, its views
// and previews. Atlas.app's executable package depends on it by path.
//
// Level 0. SwiftUI, Foundation and CoreGraphics only.
let package = Package(
  name: "AtlasUI",
  platforms: [.macOS(.v14)],
  products: [
    .library(name: "AtlasUI", targets: ["AtlasUI"])
  ],
  targets: [
    .target(name: "AtlasUI", swiftSettings: strict),
    .testTarget(name: "AtlasUITests", dependencies: ["AtlasUI"], swiftSettings: strict),
  ],
  swiftLanguageModes: [.v6]
)
