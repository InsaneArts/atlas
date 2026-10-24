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

// The rules that choose each setup step: the facts `atlas onboarding status`
// reports, the steps, the account and password rules, the agents to connect
// and the permissions the helper needs.
//
// Level 2. Depends on AtlasCommands, and its tests on AtlasService too.
// Foundation only.
let package = Package(
  name: "AtlasOnboarding",
  platforms: [.macOS(.v14)],
  products: [
    .library(name: "AtlasOnboarding", targets: ["AtlasOnboarding"]),
    .library(name: "AtlasOnboardingTesting", targets: ["AtlasOnboardingTesting"]),
  ],
  dependencies: [
    .package(path: "../AtlasCommands"),
    // AtlasService is here only for the tests, which make an `Outcome`;
    // AtlasOnboarding itself depends on AtlasCommands alone.
    .package(path: "../AtlasService"),
  ],
  targets: [
    .target(name: "AtlasOnboarding", dependencies: ["AtlasCommands"], swiftSettings: strict),
    .target(name: "AtlasOnboardingTesting", dependencies: ["AtlasOnboarding"], swiftSettings: strict),
    .testTarget(
      name: "AtlasOnboardingTests",
      dependencies: ["AtlasOnboarding", "AtlasOnboardingTesting", "AtlasCommands", "AtlasService"],
      swiftSettings: strict
    ),
  ],
  swiftLanguageModes: [.v6]
)
