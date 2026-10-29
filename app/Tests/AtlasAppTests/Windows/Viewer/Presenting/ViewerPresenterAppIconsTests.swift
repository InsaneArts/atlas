import AtlasService
import AtlasStatus
import AtlasUI
import CoreGraphics
import Testing

@testable import AtlasApp

@Suite("ViewerPresenter app icons")
struct ViewerPresenterAppIconsTests {
  /// A found icon.
  static let icon: IconImage? = {
    guard
      let context = CGContext(
        data: nil,
        width: 4,
        height: 4,
        bitsPerComponent: 8,
        bytesPerRow: 0,
        space: CGColorSpaceCreateDeviceRGB(),
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
      ),
      let image = context.makeImage()
    else { return nil }
    return IconImage(id: "notes", image: image)
  }()

  @Test("an app of known identity selects its own icon and an unknown app may use its name")
  func found() throws {
    let icon = try #require(Self.icon)
    var icons = ViewerIconsFacts()
    icons.images[AppIconKey.bundle("com.apple.Notes").text] = icon
    #expect(ViewerPresenter.appIcon(bundleId: "com.apple.Notes", name: "Notes", icons) == .image(icon))
    #expect(ViewerPresenter.appIcon(bundleId: nil, name: "Notes", icons) == .monogram(AppMonogram(name: "Notes")))
    icons.names[AppIconKey.name("notes").text] = "com.apple.Notes"
    #expect(ViewerPresenter.appIcon(bundleId: nil, name: "Notes", icons) == .image(icon))

    var byName = ViewerIconsFacts()
    byName.images[AppIconKey.name("notes").text] = icon
    #expect(ViewerPresenter.appIcon(bundleId: nil, name: "Notes", byName) == .image(icon))
  }

  @Test("an explicit bundle never borrows a same-named app's icon in either window")
  func explicitIdentityNeverBorrowsAnIcon() throws {
    let icon = try #require(Self.icon)
    let nameKey = try #require(AppIconKey(name: "Notes"))
    var icons = ViewerIconsFacts()
    icons.images[nameKey.text] = icon
    icons.images[AppIconKey.bundle("com.apple.Notes").text] = icon
    icons.names[nameKey.text] = "com.apple.Notes"
    #expect(icons.image(bundleId: "com.other.Notes", name: "Notes") == nil)
    #expect(icons.image(bundleId: nil, name: "Notes") == icon)
    #expect(icons.image(bundleId: "", name: "Notes") == icon)
    var status = ViewerStatus()
    status.overview.activity = [
      ViewerStatus.Activity(id: "app", operation: "input", ok: true, app: "Notes", bundleId: "com.other.Notes")
    ]
    let recent = PanelPresenter.recent(
      StatusReading(status),
      mode: .seen(by: status.overview, holdingControl: false),
      icons: icons.images
    )
    #expect(recent.first?.icon == .app(.monogram(AppMonogram(name: "Notes"))))
  }

  @Test("a copy only the helper can read draws its initial")
  func copyDrawsItsInitial() {
    let copy = HelperApp(
      pid: 9,
      name: "Probe",
      bundle: "/Users/atlas-helper/Library/Application Support/Atlas/Copies/Probe.app",
      bundleId: "test.atlas.probe"
    )
    let card = ViewerPresenter.appCard(copy, among: [copy], showsOrigin: true, ViewerSnapshot())
    #expect(card.icon == .monogram(AppMonogram(letter: "P", tint: MonogramTint(name: "Probe"))))
  }
}
