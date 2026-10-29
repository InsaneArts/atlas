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
  /// A copy only the helper can read.
  static let copy = "/Users/atlas-helper/Library/Application Support/Atlas/Copies/Probe.app"

  @Test("an app's icon comes from the open app, then the cache, then its bundle id, else none")
  func order() async throws {
    let folder = try TemporaryFolder()
    defer { folder.remove() }
    let live = png(width: 128, height: 128)
    let installed = png(width: 96, height: 96)
    let lookups = IconLookups()
    let icons = AppIcons(
      store: AppIconStore(caches: folder.url),
      lookup: lookups.lookup(
        icons: [Self.notes: live, "/Applications/Opie.app": installed],
        ids: ["test.opie": "/Applications/Opie.app"]
      ),
      home: Self.home
    )
    let open = HelperApp(pid: 3, name: "Notes", bundle: Self.notes, bundleId: "com.apple.Notes")

    let openWant = try #require(AppIconWant.open(open, home: Self.home, live: true))
    let fromApp = try #require(await icons.find(openWant))
    #expect(fromApp.image.width == 128)
    #expect(await lookups.asked == ["icon \(Self.notes)"])

    let notesWant = try #require(AppIconWant.named(bundleId: "com.apple.Notes", name: "Notes"))
    let cached = try #require(await icons.find(notesWant))
    #expect(cached.sha == fromApp.sha)
    #expect(await lookups.asked == ["icon \(Self.notes)"], "a kept icon asks the workspace nothing")

    let opieWant = try #require(AppIconWant.named(bundleId: "test.opie", name: "Opie"))
    let byId = try #require(await icons.find(opieWant))
    #expect(byId.image.width == 96)
    #expect(await lookups.asked.suffix(2) == ["id test.opie", "icon /Applications/Opie.app"])

    let missing = try #require(AppIconWant.named(bundleId: "test.none", name: "Nothing"))
    #expect(await icons.find(missing) == nil)
  }

  @Test("a copy only the helper can read is found only by its bundle id where the person has the app")
  func copies() async throws {
    let lookups = IconLookups()
    let icons = AppIcons(store: AppIconStore(caches: nil), lookup: lookups.lookup(), home: Self.home)
    let copy = HelperApp(pid: 9, name: "Probe", bundle: Self.copy, bundleId: "test.atlas.probe")
    let want = try #require(AppIconWant.open(copy, home: Self.home, live: true))
    #expect(want.bundle == nil, "a copy's path is never read")
    #expect(await icons.find(want) == nil)
    #expect(await lookups.asked == ["id test.atlas.probe"], "an app with a bundle id is never looked up by its name")
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
      ),
      home: Self.home
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
