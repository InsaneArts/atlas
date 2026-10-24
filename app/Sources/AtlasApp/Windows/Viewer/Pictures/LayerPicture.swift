import AtlasUI
import CoreImage
import QuartzCore
import os

/// The live picture's layer and what it shows, as ``LayerPictureView``'s coordinator: each new picture
/// replaces the layer's contents inside a transaction with actions disabled, so a frame lands whole in one
/// display tick, never crossfading from the last, and SwiftUI never rebuilds the picture's view for it. The
/// colour a look keeps is a Core Image filter on the layer; the black that darkens it lies over the view.
final class LayerPicture {
  /// Marks each swap, so Instruments shows the main thread's time per frame.
  private static let signposter = OSSignposter(subsystem: "com.insanearts.atlas", category: "Viewer")

  /// The layer the picture is drawn in, fitted at its own shape.
  let layer: CALayer
  /// What the layer shows now; nil before the first picture.
  private var shown: ViewerLivePicture?

  /// Creates the coordinator over `layer`.
  ///
  /// - Parameter layer: The layer to draw in; a new one by default.
  init(layer: CALayer = CALayer()) {
    self.layer = layer
    CATransaction.begin()
    CATransaction.setDisableActions(true)
    layer.contentsGravity = .resizeAspect
    layer.masksToBounds = true
    CATransaction.commit()
  }

  /// Shows `live`: a new picture, colour or corner replaces what the layer shows at once, with no animation; the same
  /// one again changes nothing.
  ///
  /// - Parameter live: What the slot draws.
  func show(_ live: ViewerLivePicture) {
    guard live != shown || shown?.picture.image !== live.picture.image else { return }
    let signposter = Self.signposter
    let swap = signposter.beginInterval("Frame swap")
    CATransaction.begin()
    CATransaction.setDisableActions(true)
    layer.contents = live.picture.image
    layer.cornerRadius = live.corner
    if live.saturation != shown?.saturation {
      layer.filters = Self.filters(saturation: live.saturation)
    }
    CATransaction.commit()
    shown = live
    signposter.endInterval("Frame swap", swap)
  }

  /// The filters that keep `saturation` of a picture's colour: none for all of it.
  ///
  /// - Parameter saturation: How much colour it keeps.
  /// - Returns: The layer's filters, or nil.
  private static func filters(saturation: Double) -> [CIFilter]? {
    guard saturation < 1, let filter = CIFilter(name: "CIColorControls") else { return nil }
    filter.setValue(saturation, forKey: kCIInputSaturationKey)
    return [filter]
  }
}
