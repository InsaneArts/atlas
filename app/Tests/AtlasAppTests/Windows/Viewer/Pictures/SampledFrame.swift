import AppKit
import CoreGraphics

/// Part of one drawn frame of the window, read pixel by pixel in the window's points from its top left, for following
/// how something moves from frame to frame.
struct SampledFrame {
  /// The part's pixels, in sRGB.
  let pixels: NSBitmapImageRep
  /// Where the part starts, in the window's points from its top left.
  let origin: CGPoint
  /// Pixels to a point.
  let scale: CGFloat

  /// How bright the frame draws each of `points`.
  ///
  /// - Parameter points: The points, in the window's points from its top left.
  /// - Returns: Each one's brightness.
  func brightness(at points: [CGPoint]) -> [CGFloat] {
    points.map { brightness(at: $0) }
  }

  /// How bright the frame draws `point`, from 0 for black to 1 for white; 1 outside the part read.
  ///
  /// - Parameter point: The point, in the window's points from its top left.
  /// - Returns: Its brightness.
  func brightness(at point: CGPoint) -> CGFloat {
    let x = Int((point.x - origin.x) * scale)
    let y = Int((point.y - origin.y) * scale)
    guard x >= 0, y >= 0, x < pixels.pixelsWide, y < pixels.pixelsHigh,
      let colour = pixels.colorAt(x: x, y: y)?
        .usingColorSpace(.sRGB)
    else { return 1 }
    return colour.brightnessComponent
  }

  /// How much ink the frame draws in `rect`: each sampled point's darkness, 1 less its brightness, added up every
  /// `step` points across and down, so words and pictures half faded count half.
  ///
  /// - Parameters:
  ///   - rect: The part of the frame.
  ///   - step: How far apart the points sampled are.
  /// - Returns: The ink.
  func ink(in rect: CGRect, step: CGFloat = 2) -> CGFloat {
    stride(from: rect.minY, to: rect.maxY, by: step)
      .flatMap { y in
        stride(from: rect.minX, to: rect.maxX, by: step)
          .map { CGPoint(x: $0, y: y) }
      }
      .reduce(CGFloat.zero) { $0 + (1 - brightness(at: $1)) }
  }

  /// The highest point between `top` and `bottom` where anything darker than `ink` is drawn across `columns`.
  ///
  /// - Parameters:
  ///   - columns: The points across to look at.
  ///   - top: Where to start looking down from.
  ///   - bottom: Where to stop.
  ///   - ink: Darker than this is drawn.
  /// - Returns: The point's height, or nil when nothing is drawn there.
  func firstInk(across columns: ClosedRange<CGFloat>, from top: CGFloat, to bottom: CGFloat, ink: CGFloat) -> CGFloat? {
    stride(from: top, to: bottom, by: 1)
      .first { y in
        stride(from: columns.lowerBound, through: columns.upperBound, by: 2)
          .contains { brightness(at: CGPoint(x: $0, y: y)) < ink }
      }
  }
}
