// swift-tools-version: 6.2
import PackageDescription

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
    .target(name: "AtlasService"),
    .target(name: "AtlasServiceTesting", dependencies: ["AtlasService"]),
    .testTarget(name: "AtlasServiceTests", dependencies: ["AtlasService", "AtlasServiceTesting"]),
  ],
  swiftLanguageModes: [.v6]
)
