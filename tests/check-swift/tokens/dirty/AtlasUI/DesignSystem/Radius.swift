import CoreGraphics
import SwiftUI

/// A scale with a step named after the window that uses it.
enum Radius {
  /// 8: a field, named by size.
  static let small: CGFloat = 8
  /// 22: an alert's corners, named after the alert.
  static let alert: CGFloat = 22
}

/// Shadow levels, two of them named after a surface.
enum Elevation {
  /// A picture's stage, a floating bar and the details pane's sheet, on one line.
  case stage, floating, detailsSheet(Int)
  /// A menu row's height, named after the menu.
  static var menuRow: CGFloat { 28 }
  /// The inspector's width, an instance property named after a pane.
  var inspectorWidth: CGFloat { 280 }
  /// A shadow colour written in place, not taken from Colors.swift.
  var color: Color { .black.opacity(0.5) }

  /// An app icon's sizes, a nested type named after a surface.
  enum AppIcon {
    /// The icon's side by size: allowed, since `iconSize` names the scale.
    static let iconSize: CGFloat = 64
  }
}
