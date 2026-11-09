import AtlasService
import Foundation
import Testing

@testable import AtlasApp

@Suite("AppIcons")
struct AppIconsTests {
  /// The person's home in these tests.
  static let home = "/Users/person"
  /// Notes, where it is installed.
  static let notes = "/System/Applications/Notes.app"

  @Test("a found icon is kept by its bundle id with its name, file and digest, and nothing else")
  func theDiskIndexKeepsOnlyTheIconsFacts() async throws {
    let folder = try TemporaryFolder()
    defer { folder.remove() }
    let icons = AppIcons(
      store: AppIconStore(caches: folder.url),
      lookup: IconLookups().lookup(
        icons: [Self.notes: png(width: 128, height: 128)],
        ids: ["com.apple.Notes": Self.notes]
      )
    )
    let want = try #require(AppIconWant.named(bundleId: "com.apple.Notes", name: "Notes"))
    _ = try #require(await icons.find(want))
    let index = folder.url.appendingPathComponent("AppIcons/v1/\(AppIconStore.indexName)")
    let json = try #require(try JSONSerialization.jsonObject(with: Data(contentsOf: index)) as? [String: Any])
    let entry = try #require((json["icons"] as? [String: [String: Any]])?["com.apple.Notes"])
    #expect(Set(entry.keys) == ["file", "name", "used", "read", "bytes", "sha"])
    #expect(entry["name"] as? String == "Notes")
  }

  @Test("an app's icon comes from its bundle id where it is installed, then the kept icon, else none")
  func order() async throws {
    let folder = try TemporaryFolder()
    defer { folder.remove() }
    let lookups = IconLookups()
    let icons = AppIcons(
      store: AppIconStore(caches: folder.url),
      lookup: lookups.lookup(
        icons: ["/Applications/Opie.app": png(width: 96, height: 96)],
        ids: ["test.opie": "/Applications/Opie.app"]
      )
    )
    let opieWant = try #require(AppIconWant.named(bundleId: "test.opie", name: "Opie"))
    let byId = try #require(await icons.find(opieWant))
    #expect(byId.image.width == 96)
    #expect(await lookups.asked == ["id test.opie", "icon /Applications/Opie.app"])
    let kept = try #require(await icons.find(opieWant))
    #expect(kept.sha == byId.sha)
    #expect(await lookups.asked.count == 2, "a kept icon asks the workspace nothing")

    let missing = try #require(AppIconWant.named(bundleId: "test.none", name: "Nothing"))
    #expect(await icons.find(missing) == nil)
  }

  @Test("an app known by its bundle id is never looked up by its name")
  func byIdOnly() async throws {
    let lookups = IconLookups()
    let icons = AppIcons(store: AppIconStore(caches: nil), lookup: lookups.lookup())
    let want = try #require(AppIconWant.named(bundleId: "test.atlas.probe", name: "Probe"))
    #expect(await icons.find(want) == nil)
    #expect(await lookups.asked == ["id test.atlas.probe"])
  }

  @Test("an app known only by its name is found as its bundle where apps are installed, and its name kept")
  func byName() async throws {
    let folder = try TemporaryFolder()
    defer { folder.remove() }
    let lookups = IconLookups()
    let store = AppIconStore(caches: folder.url)
    let icons = AppIcons(
      store: store,
      lookup: lookups.lookup(
        icons: [Self.notes: png(width: 128, height: 128)],
        names: ["Notes": Self.notes],
        declared: [Self.notes: "com.apple.Notes"]
      )
    )
    let want = try #require(AppIconWant.named(bundleId: nil, name: "Notes"))
    let found = try #require(await icons.find(want))
    #expect(found.bundleId == "com.apple.Notes")
    #expect(await store.bundleId(name: "notes") == "com.apple.Notes")
    #expect(await store.icon(bundleId: "com.apple.Notes") != nil)
    #expect(AppIconWant.named(bundleId: nil, name: "Probe…") == nil, "a cut name is never looked up")
    #expect(AppIconWant.named(bundleId: nil, name: "/tmp/Probe") == nil, "a path is never looked up")
  }

  @Test("an icon is read only from an app installed where apps go")
  func installedOnly() async {
    #expect(AppIconLookup.isInstalledApp("/System/Applications/Notes.app", home: Self.home))
    #expect(!AppIconLookup.isInstalledApp("/System/Applications/Notes", home: Self.home))
    #expect(!AppIconLookup.isInstalledApp("/System/Applications/Missing.app", home: Self.home))
    #expect(!AppIconLookup.isInstalledApp("/tmp/Notes.app", home: Self.home))
    #expect(await AppIconLookup.png(atBundle: "/tmp/Notes.app", home: Self.home) == nil)
  }

  @Test("a link where apps are installed draws the app it leads to, as Safari's does, never an alias")
  func links() {
    let safari = "/Applications/Safari.app"
    guard let drawn = AppIconLookup.drawnBundle(safari, home: Self.home) else {
      #expect(!FileManager.default.fileExists(atPath: safari), "an installed Safari is drawn")
      return
    }
    #expect(drawn == "/System/Cryptexes/App/System/Applications/Safari.app")
    #expect(
      AppIconLookup.drawnBundle("/System/Applications/Notes.app", home: Self.home) == "/System/Applications/Notes.app"
    )
  }

  @Test("the workspace finds an app by its bundle id only where apps are installed")
  func byWorkspaceId() async {
    #expect(await AppIconLookup.bundle(ofId: "com.apple.finder", home: Self.home) != nil)
    #expect(await AppIconLookup.bundle(ofId: "not/an id", home: Self.home) == nil)
    #expect(
      await AppIconLookup.identifier(atBundle: "/System/Library/CoreServices/Finder.app", home: Self.home)
        == "com.apple.finder"
    )
  }

  @Test("the workspace draws an installed app's icon at 128 pixels")
  func workspaceIcon() async throws {
    let png = try #require(
      await AppIconLookup.png(atBundle: "/System/Library/CoreServices/Finder.app", home: Self.home)
    )
    let image = try #require(AppIconStore.picture(png))
    #expect(image.width == AppIconCachePolicy.side)
    #expect(image.height == AppIconCachePolicy.side)
  }
}
