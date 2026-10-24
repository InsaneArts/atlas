// swift-tools-version: 6.2
import PackageDescription

// Atlas.app's face: the menu bar, the viewer and the setup window. It shows
// what Atlas's local viewer service reports and runs `atlas` commands; the
// rules that keep the helper safe live in the `atlas` command line.
//
// Levels: a target depends only on targets of a lower level. AtlasKit and
// the AtlasUI package are level 0 and never import each other; AtlasApp,
// level 1, is the only target that imports both. Local packages arrive by
// path only.
let package = Package(
  name: "Atlas",
  platforms: [.macOS(.v14)],
  products: [.executable(name: "AtlasApp", targets: ["AtlasApp"])],
  dependencies: [
    // Level 0. The design system, components, each window's state and
    // events, its views and previews: SwiftUI alone, never AtlasKit.
    .package(path: "Packages/AtlasUI")
  ],
  targets: [
    // Level 0. Models, the service client and every rule: Foundation only
    // (ServiceManagement only in Live/LoginItem.swift).
    .target(name: "AtlasKit"),
    // Level 1. The executable shipped beside `atlas` in Atlas.app: AppKit
    // windows, one controller and presenter per window, rendering, launch.
    .executableTarget(
      name: "AtlasApp",
      dependencies: ["AtlasKit", .product(name: "AtlasUI", package: "AtlasUI")]
    ),
    .testTarget(name: "AtlasKitTests", dependencies: ["AtlasKit"]),
    .testTarget(
      name: "AtlasAppTests",
      dependencies: ["AtlasApp", "AtlasKit", .product(name: "AtlasUI", package: "AtlasUI")]
    ),
  ],
  swiftLanguageModes: [.v6]
)
