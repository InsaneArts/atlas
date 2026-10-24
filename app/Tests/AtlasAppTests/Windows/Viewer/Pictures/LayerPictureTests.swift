import AtlasUI
import CoreGraphics
import QuartzCore
import Testing

@testable import AtlasApp

@Suite("LayerPicture")
struct LayerPictureTests {
  @Test("a live frame replaces the layer's contents with actions disabled")
  func aLiveFrameReplacesTheContentsWithActionsDisabled() throws {
    let layer = CALayer()
    let asked = ActionsAsked()
    layer.delegate = asked
    let picture = LayerPicture(layer: layer)
    let first = try Self.picture("one")
    picture.show(ViewerLivePicture(picture: first, saturation: 1, corner: 12))
    let second = try Self.picture("two")
    picture.show(ViewerLivePicture(picture: second, saturation: 0.5, corner: 12))
    #expect((layer.contents as AnyObject?) === second.image, "the newest frame is the layer's")
    #expect(layer.contentsGravity == .resizeAspect)
    #expect(layer.filters?.count == 1, "a paused look keeps half its colour")
    #expect(asked.keys.isEmpty, "no action was asked for: \\(asked.keys)")
  }

  /// A one-pixel picture named `id`.
  ///
  /// - Parameter id: Its id.
  /// - Returns: The picture.
  /// - Throws: The failed requirement, when the image cannot be made.
  static func picture(_ id: String) throws -> DesktopPicture {
    let context = try #require(
      CGContext(
        data: nil,
        width: 1,
        height: 1,
        bitsPerComponent: 8,
        bytesPerRow: 0,
        space: CGColorSpaceCreateDeviceRGB(),
        bitmapInfo: CGImageAlphaInfo.premultipliedFirst.rawValue
      )
    )
    return DesktopPicture(id: id, image: try #require(context.makeImage()))
  }
}

/// A layer's delegate that records each action the layer asks for, which it asks for every animatable change unless
/// actions are disabled.
private final class ActionsAsked: NSObject, CALayerDelegate {
  /// The keys asked for, in order.
  var keys: [String] = []

  func action(for layer: CALayer, forKey event: String) -> (any CAAction)? {
    keys.append(event)
    return nil
  }
}
