import Foundation
import Testing

@Suite("PackageLayout")
struct PackageLayoutTests {
  /// `app/`, found from this file's place in `app/Tests/AtlasAppTests/App/`.
  private static let app = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent()
    .deletingLastPathComponent()
    .deletingLastPathComponent()
    .deletingLastPathComponent()

  /// Each package's level, by its folder under `app/`: a package depends only on packages of a lower level.
  private static let levels = [
    "Packages/AsyncSupport": 0,
    "Packages/AtlasUI": 0,
    "Packages/AtlasService": 0,
    "Packages/AtlasCommands": 1,
    "Packages/AtlasControl": 1,
    "Packages/AtlasStatus": 1,
    "Packages/AtlasOnboarding": 2,
    ".": 3,
  ]

  /// Each manifest's text by its folder under `app/`: `app/Package.swift` as `.`, then every local package's.
  private static func manifests() throws -> [String: String] {
    let packages = app.appending(path: "Packages")
    let names = try FileManager.default
      .contentsOfDirectory(atPath: packages.path)
    var found = [".": try String(contentsOf: app.appending(path: "Package.swift"), encoding: .utf8)]
    for name in names {
      let manifest = packages.appending(path: name)
        .appending(path: "Package.swift")
      guard FileManager.default.fileExists(atPath: manifest.path) else { continue }
      found["Packages/\(name)"] = try String(contentsOf: manifest, encoding: .utf8)
    }
    return found
  }

  /// The packages `manifest` depends on, from its `.package(path:)` list, each resolved against `folder`: a folder
  /// under `app/`, or the whole resolved path when it lies outside `app/`.
  private static func dependencies(of folder: String, _ manifest: String) -> [String] {
    let root = app.standardizedFileURL.path + "/"
    return manifest.matches(of: /\.package\(path: "([^"]+)"\)/)
      .map { match in
        let resolved = app.appending(path: folder)
          .appending(path: String(match.1))
          .standardizedFileURL
          .path
        return resolved.hasPrefix(root) ? String(resolved.dropFirst(root.count)) : resolved
      }
  }

  /// The names of every Testing product the manifests declare: a library whose name ends in `Testing`.
  private static func testingProducts(_ manifests: [String: String]) -> Set<String> {
    Set(
      manifests.values.flatMap { text in
        text.matches(of: /\.library\(name: "(\w+Testing)"/)
          .map { String($0.1) }
      }
    )
  }

  /// The products and targets a target's declaration names among its dependencies, by name.
  private static func dependencyNames(_ target: Substring) -> Set<String> {
    guard let list = target.range(of: "dependencies:") else { return [] }
    let named = target[list.upperBound...]
      .matches(of: /(?:\.product\(name: |\.target\(name: |\.byName\(name: |[\[,]\s*)"(\w+)"/)
      .map { String($0.1) }
    return Set(named)
  }

  /// Each target's declaration in `manifest`, from its `.target(`, `.executableTarget(` or `.testTarget(` to the next.
  private static func targets(_ manifest: String) -> [Substring] {
    let starts = manifest.ranges(of: /\.(target|executableTarget|testTarget)\(/)
      .map(\.lowerBound)
    return starts.enumerated()
      .map { index, start in
        let end = index + 1 < starts.count ? starts[index + 1] : manifest.endIndex
        return manifest[start..<end]
      }
  }

  @Test("AtlasApp's manifest depends on AtlasUI only by local path")
  func theManifestDependsOnAtlasUIByPathOnly() throws {
    let manifests = try Self.manifests()
    let app = try #require(manifests["."])
    #expect(app.contains(".package(path: \"Packages/AtlasUI\")"))
    #expect(manifests.keys.contains("Packages/AtlasUI"), "AtlasUI is a local package")
  }

  @Test("every local package lists its dependencies by path only")
  func everyPackageListsItsDependenciesByPathOnly() throws {
    for (folder, text) in try Self.manifests() {
      let all = text.ranges(of: ".package(")
        .count
      let byPath = text.ranges(of: ".package(path: \"")
        .count
      #expect(all == byPath, "\(folder) takes a package by something other than its path")
    }
  }

  @Test("AtlasUI's manifest depends on no other package")
  func atlasUIDependsOnNoPackage() throws {
    let manifests = try Self.manifests()
    let manifest = try #require(manifests["Packages/AtlasUI"])
    #expect(!manifest.contains(".package("), "level 0 draws with SwiftUI alone")
  }

  @Test("no package depends on a package at its own level or above")
  func noPackageDependsUpward() throws {
    let manifests = try Self.manifests()
    #expect(Set(manifests.keys) == Set(Self.levels.keys), "every package has a level, and every level a package")
    for (folder, text) in manifests {
      let level = try #require(Self.levels[folder])
      #expect(text.contains("// Level \(level)."), "\(folder)'s manifest names its level, \(level)")
      for dependency in Self.dependencies(of: folder, text) {
        let below = try #require(Self.levels[dependency], "\(folder) depends on \(dependency), which has no level")
        #expect(below < level, "\(folder), level \(level), depends on \(dependency), level \(below)")
      }
    }
  }

  @Test("no shipping target depends on a Testing product")
  func noShippingTargetDependsOnATestingProduct() throws {
    let manifests = try Self.manifests()
    let testing = Self.testingProducts(manifests)
    #expect(!testing.isEmpty, "the packages declare their Testing products")
    for (folder, text) in manifests {
      let products = text.matches(of: /\.(?:library|executable)\(name: "(\w+)", targets: \[([^\]]*)\]/)
        .map { (name: String($0.1), targets: Set($0.2.matches(of: /"(\w+)"/).map { String($0.1) })) }
      let testingTargets = products.filter { testing.contains($0.name) }
        .reduce(into: Set<String>()) { $0.formUnion($1.targets) }
      for target in Self.targets(text) where !target.hasPrefix(".testTarget(") {
        let name = String(try #require(target.firstMatch(of: /name: "(\w+)"/)).1)
        guard !testingTargets.contains(name) else { continue }
        let linked = Self.dependencyNames(target)
          .intersection(testing.union(testingTargets))
        #expect(linked.isEmpty, "\(folder)'s \(name) ships, so it never links \(linked.sorted())")
      }
      for product in products where !testing.contains(product.name) {
        let exported = product.targets.intersection(testingTargets)
        #expect(exported.isEmpty, "\(folder)'s \(product.name) ships, so it never exports \(exported.sorted())")
      }
    }
  }
}
