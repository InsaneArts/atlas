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

  /// The named filter's animatable saturation, separate from frame contents and corner changes.
  static let saturationPath = "filters.colour.inputSaturation"

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

  /// Shows a new picture or corner at once, while colour eases over the same fade as the panel's dimming.
  /// An interrupted colour change continues from its presentation value; identical readings change nothing.
  ///
  /// - Parameter live: What the slot draws.
  func show(_ live: ViewerLivePicture) {
    guard live != shown || shown?.picture.image !== live.picture.image else { return }
    let colourChanged = live.saturation != shown?.saturation
    let presentation = layer.presentation()
    let previousColour = presentation?.value(forKeyPath: Self.saturationPath) as? Double ?? shown?.saturation
    let signposter = Self.signposter
    let swap = signposter.beginInterval("Frame swap")
    CATransaction.begin()
    CATransaction.setDisableActions(true)
    layer.contents = live.picture.image
    layer.cornerRadius = live.corner
    if colourChanged {
      layer.filters = Self.filters(saturation: live.saturation)
    }
    CATransaction.commit()
    if colourChanged, let previousColour {
      let animation = CABasicAnimation(keyPath: Self.saturationPath)
      animation.fromValue = previousColour
      animation.toValue = live.saturation
      animation.duration = Motion.Duration.fade.seconds
      animation.timingFunction = CAMediaTimingFunction(name: .easeOut)
      layer.add(animation, forKey: Self.saturationPath)
    }
    shown = live
    signposter.endInterval("Frame swap", swap)
  }

  /// The named colour filter, kept even at full colour so saturation always has an animatable path.
  ///
  /// - Parameter saturation: How much colour it keeps.
  /// - Returns: The layer's filter, or nil when Core Image cannot make it.
  private static func filters(saturation: Double) -> [CIFilter]? {
    guard let filter = CIFilter(name: "CIColorControls") else { return nil }
    filter.name = "colour"
    filter.setValue(saturation, forKey: kCIInputSaturationKey)
    return [filter]
  }
}
