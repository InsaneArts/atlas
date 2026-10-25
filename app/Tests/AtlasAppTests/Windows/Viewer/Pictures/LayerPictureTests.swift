import AppKit
import AtlasUI
import CoreGraphics
import QuartzCore
import SwiftUI
import Testing

@testable import AtlasApp

@Suite("LayerPicture", .serialized, AppKitState())
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

  @Test("pause and resume ease saturation independently while new frames still replace contents immediately")
  func pauseEasesColourOnly() throws {
    let layer = CALayer()
    let picture = LayerPicture(layer: layer)
    let first = try Self.picture("one")
    picture.show(ViewerLivePicture(picture: first, saturation: 1, corner: 12))
    #expect(layer.animationKeys() == nil, "the first picture lands immediately")
    for saturation in [0.5, 1.0] {
      picture.show(ViewerLivePicture(picture: first, saturation: saturation, corner: 12))
      let animation = try #require(layer.animation(forKey: LayerPicture.saturationPath) as? CABasicAnimation)
      #expect(animation.toValue as? Double == saturation)
      #expect(animation.duration == Motion.Duration.fade.seconds)
      #expect(animation.timingFunction == CAMediaTimingFunction(name: .easeOut))
      let next = try Self.picture("next")
      picture.show(ViewerLivePicture(picture: next, saturation: saturation, corner: 12))
      #expect((layer.contents as AnyObject?) === next.image)
      #expect(layer.animationKeys() == [LayerPicture.saturationPath])
      #expect(layer.animation(forKey: "contents") == nil)
    }
  }

  @Test("the window's live slot shows each new frame through its layer, without building its view again")
  @MainActor
  func theLiveSlotShowsEachNewFrame() throws {
    let first = try Self.picture("one")
    let live = ViewerLivePicture(picture: first, saturation: 1, corner: 12)
    let hosting = NSHostingView(rootView: LayerPictureView(live: live))
    hosting.frame = NSRect(x: 0, y: 0, width: 160, height: 100)
    hosting.layoutSubtreeIfNeeded()
    let slot = try #require(Self.pictureViews(in: hosting).first, "the slot hosts no layer")
    #expect((slot.layer?.contents as AnyObject?) === first.image)
    let second = try Self.picture("two")
    hosting.rootView = LayerPictureView(live: ViewerLivePicture(picture: second, saturation: 1, corner: 12))
    hosting.layoutSubtreeIfNeeded()
    #expect(Self.pictureViews(in: hosting).count == 1)
    #expect(Self.pictureViews(in: hosting).first === slot, "the slot's view was built again")
    #expect((slot.layer?.contents as AnyObject?) === second.image, "the new frame never reached the layer")
  }

  /// The views under `view` whose layer shows a picture.
  ///
  /// - Parameter view: A view.
  /// - Returns: Those views, in tree order.
  @MainActor
  static func pictureViews(in view: NSView) -> [NSView] {
    let own = view.layer?.contents == nil ? [] : [view]
    return own + view.subviews.flatMap { pictureViews(in: $0) }
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
