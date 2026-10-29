import SwiftUI

/// What a surface floating over the window's ground is: each an opaque fill, never glass or a material, so nothing
/// behind it shows through and Reduce Transparency has nothing to change. Names describe the role so each fill can be
/// shared by several surfaces.
enum SurfaceStyle: Sendable, CaseIterable {
  /// What floats over the stage or a picture: the large screenshot's bar, and the panel where no popover holds it.
  case floating
  /// A modal surface over the scrim, such as an alert.
  case modal

  /// The surface's opaque fill.
  var fill: Color {
    switch self {
    case .floating: .fillFloating
    case .modal: .fillModal
    }
  }
}
