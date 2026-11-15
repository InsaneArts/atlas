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

// Atlas VM's rules: a VM's folder and settings, its power, the changes its
// station asks for, kept before they are done, and when its window may have
// input, in AtlasVMKit, which has no Virtualization; and AtlasVM, the program
// that runs one VM with Apple's framework and gives those rules a host.
//
// Level 0. Foundation only.
let package = Package(
  name: "AtlasVM",
  platforms: [.macOS(.v14)],
  products: [
    .library(name: "AtlasVMKit", targets: ["AtlasVMKit"]),
    .executable(name: "AtlasVM", targets: ["AtlasVM"]),
  ],
  targets: [
    .target(name: "AtlasVMKit", swiftSettings: strict),
    // The thin program: Apple's framework, the window, and the sockets.
    .executableTarget(
      name: "AtlasVM",
      dependencies: ["AtlasVMKit"],
      swiftSettings: strict,
      linkerSettings: [.linkedFramework("Virtualization")]
    ),
    .testTarget(name: "AtlasVMKitTests", dependencies: ["AtlasVMKit"], swiftSettings: strict),
  ],
  swiftLanguageModes: [.v6]
)
