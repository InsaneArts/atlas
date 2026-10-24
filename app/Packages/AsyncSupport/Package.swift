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

// The shared waits and the manual clock of every Atlas test target, as a
// library only test targets depend on. Nothing here ships.
//
// Level 0. Testing, Observation and os only.
let package = Package(
  name: "AsyncSupport",
  platforms: [.macOS(.v14)],
  products: [
    .library(name: "AsyncSupportTesting", targets: ["AsyncSupportTesting"])
  ],
  targets: [
    .target(name: "AsyncSupportTesting", swiftSettings: strict),
    .testTarget(
      name: "AsyncSupportTests",
      dependencies: ["AsyncSupportTesting"],
      swiftSettings: strict
    ),
  ],
  swiftLanguageModes: [.v6]
)
