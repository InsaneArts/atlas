// swift-tools-version: 6.2
import PackageDescription

// The shared waits of every Atlas test target, as a library only test
// targets depend on. Nothing here ships.
//
// Level 0. Foundation and Testing only.
let package = Package(
  name: "AsyncSupport",
  platforms: [.macOS(.v14)],
  products: [
    .library(name: "AsyncSupportTesting", targets: ["AsyncSupportTesting"])
  ],
  targets: [
    .target(name: "AsyncSupportTesting")
  ],
  swiftLanguageModes: [.v6]
)
