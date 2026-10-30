import AppKit
import AtlasUI
import SwiftUI

extension NSColor {
  /// The ground behind every window's content, so a resize never flashes another colour: AtlasUI's `Color.ground`,
  /// light or dark as the window's appearance is.
  static let ground = NSColor(Color.ground)
}
