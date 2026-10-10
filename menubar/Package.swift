// swift-tools-version: 6.0
import PackageDescription

// Understudy's menu bar app. It shows what the local viewer reports and runs
// `understudy` commands; it holds no state or policy of its own.
let package = Package(
    name: "UnderstudyBar",
    platforms: [.macOS(.v14)],
    products: [.executable(name: "UnderstudyBar", targets: ["UnderstudyBar"])],
    targets: [
        .target(name: "UnderstudyKit"),
        .target(name: "UnderstudyUI", dependencies: ["UnderstudyKit"]),
        .executableTarget(name: "UnderstudyBar", dependencies: ["UnderstudyKit", "UnderstudyUI"]),
        .testTarget(name: "UnderstudyKitTests", dependencies: ["UnderstudyKit"]),
    ],
    swiftLanguageModes: [.v5]
)
