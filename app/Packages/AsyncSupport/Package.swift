// swift-tools-version: 6.2
import PackageDescription

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
    .target(name: "AsyncSupportTesting"),
    .testTarget(name: "AsyncSupportTests", dependencies: ["AsyncSupportTesting"]),
  ],
  swiftLanguageModes: [.v6]
)
