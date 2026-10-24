import AtlasService
import Foundation
import Testing

@testable import AtlasControl

@Suite("DesktopPoint+Mapping")
struct DesktopPointMappingTests {
  @Test("a point maps through the letterboxed picture to display points")
  func letterboxed() {
    let view = CGSize(width: 400, height: 400)
    let image = CGSize(width: 1600, height: 900)
    let display = ViewerStatus.Size(width: 1280, height: 720)
    // The picture is letterboxed: 400 × 225, from y 87.5.
    #expect(DesktopPoint(location: CGPoint(x: 80, y: 10), in: view, image: image, display: display) == nil)
    #expect(
      DesktopPoint(location: CGPoint(x: 200, y: 200), in: view, image: image, display: display)
        == DesktopPoint(x: 640, y: 360)
    )
    #expect(DesktopPoint(location: CGPoint(x: 480, y: 170), in: view, image: image, display: display) == nil)
  }

  @Test(
    "a drag that leaves the picture is clamped to its edge, a thousandth of a view point inside the far ones",
    arguments: [
      (CGPoint(x: -10, y: 70), DesktopPoint(x: 0, y: 280)),
      (CGPoint(x: 410, y: 70), DesktopPoint(x: 1599.996, y: 280)),
      (CGPoint(x: 80, y: -20), DesktopPoint(x: 320, y: 0)),
      (CGPoint(x: 80, y: 320), DesktopPoint(x: 320, y: 1199.996)),
    ] as [(CGPoint, DesktopPoint)]
  )
  func clamped(location: CGPoint, edge: DesktopPoint) throws {
    let view = CGSize(width: 400, height: 300)
    let image = CGSize(width: 800, height: 600)
    let display = ViewerStatus.Size(width: 1600, height: 1200)
    #expect(DesktopPoint(location: location, in: view, image: image, display: display) == nil)
    let point = try #require(DesktopPoint(location: location, in: view, image: image, display: display, clamp: true))
    #expect(point.distance(to: edge) < 1e-9)
  }

  @Test("the view's scale is the letterboxed picture's width over the display's, whatever the picture's pixels")
  func scale() {
    let image = CGSize(width: 1600, height: 900)
    let display = ViewerStatus.Size(width: 1280, height: 720)
    // Letterboxed to 400 wide in a square view, and held to 320 wide by a short one: 1280 display points either way.
    #expect(DesktopPoint.scale(in: CGSize(width: 400, height: 400), image: image, display: display) == 0.3125)
    #expect(DesktopPoint.scale(in: CGSize(width: 800, height: 180), image: image, display: display) == 0.25)
    #expect(DesktopPoint.scale(in: CGSize(width: 0, height: 0), image: image, display: display) == nil)
  }

  @Test("an empty size maps nowhere")
  func emptySize() {
    #expect(
      DesktopPoint(
        location: CGPoint(x: 0, y: 0),
        in: CGSize(width: 0, height: 0),
        image: CGSize(width: 1, height: 1),
        display: .init(width: 1, height: 1)
      )
        == nil
    )
  }
}
