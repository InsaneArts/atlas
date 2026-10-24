// swift-tools-version: 6.2
import PackageDescription

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
    .target(name: "AtlasOnboarding", dependencies: ["AtlasCommands"]),
    .target(name: "AtlasOnboardingTesting", dependencies: ["AtlasOnboarding"]),
    .testTarget(
      name: "AtlasOnboardingTests",
      dependencies: ["AtlasOnboarding", "AtlasOnboardingTesting", "AtlasCommands", "AtlasService"]
    ),
  ],
  swiftLanguageModes: [.v6]
)
