import Foundation
import Testing

@testable import AtlasKit

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
    "a drag that leaves the picture is clamped to its edge",
    arguments: [CGPoint(x: -10, y: 70), CGPoint(x: 410, y: 70), CGPoint(x: 80, y: -20), CGPoint(x: 80, y: 320)]
  )
  func clamped(location: CGPoint) {
    let view = CGSize(width: 400, height: 300)
    let image = CGSize(width: 800, height: 600)
    let display = ViewerStatus.Size(width: 1600, height: 1200)
    #expect(DesktopPoint(location: location, in: view, image: image, display: display) == nil)
    let point = DesktopPoint(location: location, in: view, image: image, display: display, clamp: true)
    #expect(point.map { $0.x >= 0 && $0.x < 1600 && $0.y >= 0 && $0.y < 1200 } == true)
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
