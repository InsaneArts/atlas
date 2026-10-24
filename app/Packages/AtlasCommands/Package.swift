// swift-tools-version: 6.2
import PackageDescription

// What Atlas.app runs or reads on this Mac outside the viewer service: the
// `atlas` command runner and the replies the app reads, the agents' status,
// the login item and the login records.
//
// Level 1. Depends on AtlasService. Foundation, plus ServiceManagement in
// Live/LoginItem.swift alone.
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
    .target(name: "AtlasCommands", dependencies: ["AtlasService"]),
    .target(name: "AtlasCommandsTesting", dependencies: ["AtlasCommands", "AtlasService"]),
    .testTarget(
      name: "AtlasCommandsTests",
      dependencies: [
        "AtlasCommands", "AtlasService",
        .product(name: "AsyncSupportTesting", package: "AsyncSupport"),
        .product(name: "AtlasServiceTesting", package: "AtlasService"),
      ]
    ),
  ],
  swiftLanguageModes: [.v6]
)
