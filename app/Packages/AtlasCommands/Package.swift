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

// What Atlas.app runs or reads on this Mac outside the viewer service: the
// `atlas` command runner and the replies the app reads, the agents' status,
// the login item and the login records.
//
// Level 1. Depends on AtlasService. Foundation, plus os for the locks in
// Live/ and ServiceManagement in Live/LoginItem.swift alone.
let package = Package(
  name: "AtlasCommands",
  platforms: [.macOS(.v14)],
  products: [
    .library(name: "AtlasCommands", targets: ["AtlasCommands"]),
    .library(name: "AtlasCommandsTesting", targets: ["AtlasCommandsTesting"]),
  ],
  dependencies: [
    // AsyncSupport is here only for AsyncSupportTesting, whose Ceiling the
    // tests wait through; AtlasCommands itself depends on AtlasService alone.
    .package(path: "../AsyncSupport"),
    .package(path: "../AtlasService"),
  ],
  targets: [
    .target(name: "AtlasCommands", dependencies: ["AtlasService"], swiftSettings: strict),
    .target(
      name: "AtlasCommandsTesting",
      dependencies: ["AtlasCommands", "AtlasService"],
      swiftSettings: strict
    ),
    .testTarget(
      name: "AtlasCommandsTests",
      dependencies: [
        "AtlasCommands", "AtlasService",
        .product(name: "AsyncSupportTesting", package: "AsyncSupport"),
        .product(name: "AtlasServiceTesting", package: "AtlasService"),
      ],
      swiftSettings: strict
    ),
  ],
  swiftLanguageModes: [.v6]
)
