import AtlasService
import Foundation
import Testing

@testable import AtlasApp

@Suite("AppIconStore")
struct AppIconStoreTests {
  /// The day these tests run on.
  nonisolated static let today = 20_400

  @Test("an icon once read is kept across launches")
  func keptAcrossLaunches() async throws {
    let folder = try TemporaryFolder()
    defer { folder.remove() }
    let icon = png(width: 128, height: 128)
    let first = AppIconStore(caches: folder.url) { Self.today }
    #expect(await first.keep(icon, bundleId: "com.apple.Notes", name: "Notes") != nil)
    await first.learn(name: "notes", bundleId: "com.apple.Notes")

    let next = AppIconStore(caches: folder.url) { Self.today }
    let kept = try #require(await next.icon(bundleId: "com.apple.Notes"))
    #expect(kept.image.width == 128)
    #expect(!kept.looksAgain)
    #expect(await next.bundleId(name: "notes") == "com.apple.Notes")

    let store = folder.url.appendingPathComponent("AppIcons/v1")
    #expect(try Self.mode(of: store) == 0o700)
    for file in try FileManager.default.contentsOfDirectory(atPath: store.path) {
      #expect(try Self.mode(of: store.appendingPathComponent(file)) == 0o600, "\(file)")
      #expect(file == AppIconStore.indexName || file.hasSuffix(".png"), "\(file)")
    }
  }

  @Test("a broken index starts the cache over")
  func brokenIndex() async throws {
    let folder = try TemporaryFolder()
    defer { folder.remove() }
    let first = AppIconStore(caches: folder.url) { Self.today }
    await first.keep(png(width: 128, height: 128), bundleId: "com.apple.Notes", name: "Notes")
    let store = folder.url.appendingPathComponent("AppIcons/v1")
    try Data("{ not json".utf8)
      .write(to: store.appendingPathComponent(AppIconStore.indexName))

    let next = AppIconStore(caches: folder.url) { Self.today }
    #expect(await next.icon(bundleId: "com.apple.Notes") == nil)
    let left = try FileManager.default
      .contentsOfDirectory(atPath: store.path)
    #expect(left.allSatisfy { !$0.hasSuffix(".png") }, "\(left)")
  }

  @Test("the cache keeps no time of use finer than a day, and nothing but a name, an icon and its days")
  func dayOnly() async throws {
    let folder = try TemporaryFolder()
    defer { folder.remove() }
    let store = AppIconStore(caches: folder.url) { Self.today }
    await store.keep(png(width: 128, height: 128), bundleId: "com.apple.Notes", name: "Notes")
    let index = folder.url.appendingPathComponent("AppIcons/v1/\(AppIconStore.indexName)")
    let json = try #require(
      try JSONSerialization.jsonObject(with: Data(contentsOf: index)) as? [String: Any]
    )
    #expect(Set(json.keys) == ["version", "icons", "names"])
    let entry = try #require((json["icons"] as? [String: [String: Any]])?["com.apple.Notes"])
    #expect(Set(entry.keys) == ["file", "name", "used", "read", "bytes", "sha"])
    #expect(entry["used"] as? Int == Self.today)
    #expect(entry["read"] as? Int == Self.today)
  }

  @Test("a live icon that changed replaces the cached one")
  func changedIcon() async throws {
    let folder = try TemporaryFolder()
    defer { folder.remove() }
    let store = AppIconStore(caches: folder.url) { Self.today }
    let old = try #require(await store.keep(png(width: 64, height: 64), bundleId: "com.apple.Notes", name: "Notes"))
    let new = try #require(await store.keep(png(width: 128, height: 128), bundleId: "com.apple.Notes", name: "Notes"))
    #expect(old.sha != new.sha)

    let next = AppIconStore(caches: folder.url) { Self.today }
    let kept = try #require(await next.icon(bundleId: "com.apple.Notes"))
    #expect(kept.sha == new.sha)
    #expect(kept.image.width == 128)
    let pictures = try FileManager.default
      .contentsOfDirectory(atPath: folder.url.appendingPathComponent("AppIcons/v1").path)
      .filter { $0.hasSuffix(".png") }
    #expect(pictures.count == 1)
  }

  @Test("an icon kept thirty days ago is due to be looked up again")
  func dueAgain() async throws {
    let folder = try TemporaryFolder()
    defer { folder.remove() }
    let first = AppIconStore(caches: folder.url) { Self.today }
    await first.keep(png(width: 128, height: 128), bundleId: "com.apple.Notes", name: "Notes")
    let later = AppIconStore(caches: folder.url) { Self.today + AppIconCachePolicy.lookAgainDays }
    #expect(await later.icon(bundleId: "com.apple.Notes")?.looksAgain == true)
  }

  @Test("an icon too large, too wide or not a picture is refused")
  func refused() async throws {
    let folder = try TemporaryFolder()
    defer { folder.remove() }
    let store = AppIconStore(caches: folder.url) { Self.today }
    let wide = png(width: AppIconCachePolicy.largestSide + 1, height: 16)
    #expect(await store.keep(wide, bundleId: "com.apple.Wide", name: nil) == nil)
    #expect(await store.keep(Data("not a png".utf8), bundleId: "com.apple.Text", name: nil) == nil)
    let large = Data(count: AppIconCachePolicy.largestFile + 1)
    #expect(await store.keep(large, bundleId: "com.apple.Large", name: nil) == nil)
    #expect(await store.keep(png(width: 128, height: 128), bundleId: "not/an id", name: nil) == nil)
  }

  @Test("a link in the way is never followed, and the rest of Atlas's caches are left as they were")
  func links() async throws {
    let folder = try TemporaryFolder()
    let elsewhere = try TemporaryFolder()
    defer {
      folder.remove()
      elsewhere.remove()
    }
    let lock = folder.url.appendingPathComponent("helper.lock")
    try Data()
      .write(to: lock)
    try FileManager.default
      .createSymbolicLink(
        at: folder.url.appendingPathComponent("AppIcons"),
        withDestinationURL: elsewhere.url
      )
    let store = AppIconStore(caches: folder.url) { Self.today }
    #expect(await store.keep(png(width: 128, height: 128), bundleId: "com.apple.Notes", name: "Notes") != nil)
    #expect(await store.icon(bundleId: "com.apple.Notes") == nil, "nothing was kept through the link")
    #expect(try FileManager.default.contentsOfDirectory(atPath: elsewhere.url.path).isEmpty)
    #expect(FileManager.default.fileExists(atPath: lock.path))
  }

  @Test(
    "a symlink at any cache ancestor is refused before reading, writing or cleaning",
    arguments: ["Library", "Caches", "Atlas"]
  )
  func linkedAncestorsAreRefused(component: String) async throws {
    let root = try TemporaryFolder()
    let elsewhere = try TemporaryFolder()
    defer {
      root.remove()
      elsewhere.remove()
    }
    let components = ["Library", "Caches", "Atlas"]
    let position = try #require(components.firstIndex(of: component))
    let parent = components.prefix(position)
      .reduce(root.url) { $0.appendingPathComponent($1) }
    try FileManager.default
      .createDirectory(at: parent, withIntermediateDirectories: true)
    let target = components.dropFirst(position + 1)
      .reduce(elsewhere.url) { $0.appendingPathComponent($1) }
      .appendingPathComponent("AppIcons/v1")
    try FileManager.default
      .createDirectory(at: target, withIntermediateDirectories: true)
    let sentinel = target.appendingPathComponent("untouched")
    try Data("keep me".utf8)
      .write(to: sentinel)
    try FileManager.default
      .createSymbolicLink(at: parent.appendingPathComponent(component), withDestinationURL: elsewhere.url)
    let caches = root.url.appendingPathComponent("Library/Caches/Atlas")
    #expect(AppIconStore.made(caches) == nil)
    let store = AppIconStore(caches: caches)
    await store.keep(png(width: 64, height: 64), bundleId: "com.apple.Notes", name: "Notes")
    await store.learn(name: "notes", bundleId: "com.apple.Notes")
    #expect(await store.icon(bundleId: "com.apple.Notes") == nil)
    #expect(try FileManager.default.contentsOfDirectory(atPath: target.path) == ["untouched"])
    #expect(try Data(contentsOf: sentinel) == Data("keep me".utf8))
  }

  @Test("a verified cache handle cannot be redirected by replacing an ancestor")
  func ancestorReplacementCannotRedirectOperations() throws {
    let root = try TemporaryFolder()
    let elsewhere = try TemporaryFolder()
    defer {
      root.remove()
      elsewhere.remove()
    }
    let ancestor = root.url.appendingPathComponent("Library")
    let caches = ancestor.appendingPathComponent("Caches/Atlas")
    let directory = try #require(AppIconStore.made(caches))
    let original = root.url.appendingPathComponent("original")
    try FileManager.default
      .moveItem(at: ancestor, to: original)
    try FileManager.default
      .createSymbolicLink(at: ancestor, withDestinationURL: elsewhere.url)
    let redirected = elsewhere.url.appendingPathComponent("Caches/Atlas/AppIcons/v1")
    try FileManager.default
      .createDirectory(at: redirected, withIntermediateDirectories: true)
    let sentinel = redirected.appendingPathComponent("untouched")
    try Data("outside".utf8)
      .write(to: sentinel)

    #expect(directory.write(Data("inside".utf8), name: "kept"))
    #expect(directory.read("kept", largest: 16) == Data("inside".utf8))
    #expect(directory.write(Data(), name: "stray"))
    AppIconStore.sweep(directory, keeping: ["kept"])
    #expect(Set(directory.names) == ["kept"])
    AppIconStore.empty(directory)
    #expect(directory.names.isEmpty)
    #expect(try Data(contentsOf: sentinel) == Data("outside".utf8))
    #expect(try FileManager.default.contentsOfDirectory(atPath: redirected.path) == ["untouched"])
  }

  @Test("leaf links cannot be read and replacing one never writes to its target")
  func leafLinksCannotRedirectFiles() throws {
    let root = try TemporaryFolder()
    defer { root.remove() }
    let target = root.url.appendingPathComponent("target")
    try Data("outside".utf8)
      .write(to: target)
    let directory = try #require(AppIconStore.made(root.url))
    let leaf = root.url.appendingPathComponent("AppIcons/v1/leaf")
    try FileManager.default
      .createSymbolicLink(at: leaf, withDestinationURL: target)
    #expect(directory.read("leaf", largest: 16) == nil)
    #expect(directory.write(Data("inside".utf8), name: "leaf"))
    #expect(directory.read("leaf", largest: 16) == Data("inside".utf8))
    #expect(try Data(contentsOf: target) == Data("outside".utf8))
    #expect(!directory.write(Data(), name: "../target"))
  }

  /// A file's permissions.
  ///
  /// - Parameter url: The file.
  /// - Returns: Its permission bits.
  /// - Throws: When it cannot be read.
  private static func mode(of url: URL) throws -> Int {
    let attributes = try FileManager.default
      .attributesOfItem(atPath: url.path)
    return (attributes[.posixPermissions] as? Int ?? 0) & 0o777
  }
}
