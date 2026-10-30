import AtlasService
import AtlasStatus
import AtlasUI
import CoreGraphics
import Testing

@testable import AtlasApp

@Suite("PanelPresenter Recent")
struct PanelPresenterRecentTests {
  @Test("a Recent row leads with its app's icon, Atlas's for Atlas's own work, and the person's mark for theirs")
  func aRecentRowLeadsWithItsIcon() throws {
    var status = ViewerStatus()
    status.overview.activity = [
      ViewerStatus.Activity(
        id: "a",
        operation: "input",
        ok: true,
        ageSeconds: 1,
        says: "clicked Save in TextEdit",
        app: "TextEdit",
        bundleId: "com.apple.TextEdit"
      ),
      ViewerStatus.Activity(id: "b", operation: "restart", ok: true, ageSeconds: 2, says: "recovered the helper"),
      ViewerStatus.Activity(
        id: "c",
        operation: "input",
        ok: true,
        actor: "viewer",
        ageSeconds: 3,
        says: "clicked in Opie",
        app: "Opie"
      ),
    ]
    let found = try #require(Self.image)
    let key = try #require(AppIconKey(bundleId: "com.apple.TextEdit"))
    let rows = PanelPresenter.recent(
      StatusReading(status),
      mode: .seen(by: status.overview, holdingControl: false),
      icons: [key.text: found]
    )
    #expect(rows.map(\.icon) == [.app(.image(found)), .atlas, .you])
    let missing = PanelPresenter.recent(
      StatusReading(status),
      mode: .seen(by: status.overview, holdingControl: false),
      icons: [:]
    )
    #expect(
      missing.first?.icon == .app(.monogram(AppMonogram(name: "TextEdit"))),
      "an icon not found yet draws its tile"
    )
  }

  @Test("a Recent row in no app leads with a quiet tile of what it did")
  func aRecentRowInNoAppLeadsWithAQuietTile() {
    var status = ViewerStatus()
    status.overview.activity = [
      ViewerStatus.Activity(id: "a", operation: "screenshot", ok: true, ageSeconds: 1, says: "took a screenshot"),
      ViewerStatus.Activity(id: "b", operation: "run", ok: true, ageSeconds: 2, says: "ran a command"),
    ]
    let rows = PanelPresenter.recent(
      StatusReading(status),
      mode: .seen(by: status.overview, holdingControl: false),
      icons: [:]
    )
    #expect(rows.map(\.icon) == [.symbol(.camera), .symbol(.terminal)])
  }

  /// A one-pixel picture standing for an icon found.
  static var image: IconImage? {
    let context = CGContext(
      data: nil,
      width: 1,
      height: 1,
      bitsPerComponent: 8,
      bytesPerRow: 4,
      space: CGColorSpaceCreateDeviceRGB(),
      bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
    )
    return context?.makeImage()
      .map { IconImage(id: "TextEdit", image: $0) }
  }
}
