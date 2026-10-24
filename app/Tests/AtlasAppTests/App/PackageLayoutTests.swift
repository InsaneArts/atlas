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

  @Test("AtlasApp's manifest depends on AtlasUI only by local path")
  func theManifestDependsOnAtlasUIByPathOnly() throws {
    let manifests = try Self.manifests()
    let app = try #require(manifests["."])
    #expect(app.contains(".package(path: \"Packages/AtlasUI\")"))
    #expect(manifests.keys.contains("Packages/AtlasUI"), "AtlasUI is a local package")
  }

  @Test("no manifest in app/ takes a package from a URL")
  func noManifestTakesAPackageFromAURL() throws {
    for (folder, text) in try Self.manifests() {
      let remote = text.range(of: #"\.package\([^)]*\burl:"#, options: .regularExpression)
      #expect(remote == nil, "\(folder) takes a package from a URL")
    }
  }

  @Test("AtlasUI's manifest depends on no other package")
  func atlasUIDependsOnNoPackage() throws {
    let manifests = try Self.manifests()
    let manifest = try #require(manifests["Packages/AtlasUI"])
    #expect(!manifest.contains(".package("), "level 0 draws with SwiftUI alone")
  }
}
