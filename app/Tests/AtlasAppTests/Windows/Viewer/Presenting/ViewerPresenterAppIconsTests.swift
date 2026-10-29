import AtlasService
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

  @Test("an app's icon is the one found by its bundle id, then by the bundle its name was seen with, then by its name")
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
