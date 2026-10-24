import AppKit
import SwiftUI

/// The colours a hosted viewer's live slot paints each picture by its id (``ViewerHost``), so a frame read shows where
/// each picture stands.
enum PictureColour: String, CaseIterable {
  /// The stage's picture, red.
  case stage
  /// The corner's picture, blue.
  case corner

  /// The colour.
  var color: Color {
    switch self {
    case .stage: Color(red: 1, green: 0, blue: 0)
    case .corner: Color(red: 0, green: 0, blue: 1)
    }
  }

  /// Whether `colour` is this one, drawn whole or nearly.
  ///
  /// - Parameter colour: A drawn colour.
  /// - Returns: Whether it matches.
  func matches(_ colour: NSColor) -> Bool {
    switch self {
    case .stage: colour.redComponent > 0.8 && colour.blueComponent < 0.2
    case .corner: colour.blueComponent > 0.8 && colour.redComponent < 0.2
    }
  }
}
