// swift-tools-version: 6.2
import PackageDescription

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
    .target(name: "AtlasUI"),
    .testTarget(name: "AtlasUITests", dependencies: ["AtlasUI"]),
  ],
  swiftLanguageModes: [.v6]
)
