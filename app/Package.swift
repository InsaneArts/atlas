// swift-tools-version: 6.2
import PackageDescription

/// Swift 6 language mode makes strict concurrency checking complete. The
/// app runs on the main thread, so its targets default to MainActor and mark
/// what leaves it `nonisolated`. The two upcoming features are Approachable
/// Concurrency: a nonisolated async function runs on its caller's actor
/// unless marked `@concurrent`, and a conformance of an isolated type is
/// inferred to share its isolation.
let mainActor: [SwiftSetting] = [
  .defaultIsolation(MainActor.self),
  .enableUpcomingFeature("NonisolatedNonsendingByDefault"),
  .enableUpcomingFeature("InferIsolatedConformances"),
]

// Atlas.app's face: the menu bar, the viewer and the setup window. It shows
// what Atlas's local viewer service reports and runs `atlas` commands; the
// rules that keep the helper safe live in the `atlas` command line.
//
// Levels: a package depends only on packages of a lower level, and local
// packages arrive by path only. Level 0: AsyncSupport, AtlasUI and
// AtlasService. Level 1: AtlasCommands, AtlasControl and AtlasStatus, on
// AtlasService. Level 2: AtlasOnboarding, on AtlasCommands. AtlasUI never
// sees a domain package, and no domain package sees AtlasUI.
//
// Level 3. The executable: AtlasApp is the only target that imports them
// all. No shipping target depends on a `…Testing` product; only
// AtlasAppTests does.
let package = Package(
  name: "Atlas",
  platforms: [.macOS(.v14)],
  products: [.executable(name: "AtlasApp", targets: ["AtlasApp"])],
  dependencies: [
    .package(path: "Packages/AsyncSupport"),
    .package(path: "Packages/AtlasCommands"),
    .package(path: "Packages/AtlasControl"),
    .package(path: "Packages/AtlasOnboarding"),
    .package(path: "Packages/AtlasService"),
    .package(path: "Packages/AtlasStatus"),
    .package(path: "Packages/AtlasUI"),
  ],
  targets: [
    // The executable shipped beside `atlas` in Atlas.app: AppKit windows,
    // one controller and presenter per window, rendering, launch.
    .executableTarget(
      name: "AtlasApp",
      dependencies: [
        .product(name: "AtlasCommands", package: "AtlasCommands"),
        .product(name: "AtlasControl", package: "AtlasControl"),
        .product(name: "AtlasOnboarding", package: "AtlasOnboarding"),
        .product(name: "AtlasService", package: "AtlasService"),
        .product(name: "AtlasStatus", package: "AtlasStatus"),
        .product(name: "AtlasUI", package: "AtlasUI"),
      ],
      swiftSettings: mainActor
    ),
    .testTarget(
      name: "AtlasAppTests",
      dependencies: [
        "AtlasApp",
        .product(name: "AsyncSupportTesting", package: "AsyncSupport"),
        .product(name: "AtlasCommands", package: "AtlasCommands"),
        .product(name: "AtlasCommandsTesting", package: "AtlasCommands"),
        .product(name: "AtlasControl", package: "AtlasControl"),
        .product(name: "AtlasControlTesting", package: "AtlasControl"),
        .product(name: "AtlasOnboarding", package: "AtlasOnboarding"),
        .product(name: "AtlasOnboardingTesting", package: "AtlasOnboarding"),
        .product(name: "AtlasService", package: "AtlasService"),
        .product(name: "AtlasServiceTesting", package: "AtlasService"),
        .product(name: "AtlasStatus", package: "AtlasStatus"),
        .product(name: "AtlasUI", package: "AtlasUI"),
      ],
      swiftSettings: mainActor
    ),
  ],
  swiftLanguageModes: [.v6]
)
