import SwiftUI

/// One side of a colour token: an sRGB colour and its opacity, as the token's documentation writes it.
struct ColorValue: Sendable, Equatable {
  /// The red channel, 0 to 1.
  let red: Double
  /// The green channel, 0 to 1.
  let green: Double
  /// The blue channel, 0 to 1.
  let blue: Double
  /// The opacity, 0 to 1.
  let alpha: Double

  /// A colour from its hex value, such as `0xF5F5F7`, at `alpha`.
  ///
  /// - Parameters:
  ///   - hex: The colour as `0xRRGGBB`.
  ///   - alpha: Its opacity; whole by default.
  /// - Returns: The colour.
  static func rgb(_ hex: UInt32, alpha: Double = 1) -> ColorValue {
    ColorValue(
      red: Double((hex >> 16) & 0xFF) / 255,
      green: Double((hex >> 8) & 0xFF) / 255,
      blue: Double(hex & 0xFF) / 255,
      alpha: alpha
    )
  }

  /// White at `alpha`, for a fill or an edge that lightens what is under it.
  ///
  /// - Parameter alpha: Its opacity.
  /// - Returns: The colour.
  static func white(_ alpha: Double) -> ColorValue {
    rgb(0xFFFFFF, alpha: alpha)
  }

  /// Black at `alpha`, for a fill, an edge or a shadow that darkens what is under it.
  ///
  /// - Parameter alpha: Its opacity.
  /// - Returns: The colour.
  static func black(_ alpha: Double) -> ColorValue {
    rgb(0x000000, alpha: alpha)
  }

  /// The colour as AppKit draws it.
  var nsColor: NSColor {
    NSColor(srgbRed: red, green: green, blue: blue, alpha: alpha)
  }
}
