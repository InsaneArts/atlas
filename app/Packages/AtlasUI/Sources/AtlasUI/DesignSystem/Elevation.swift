import SwiftUI

/// How far a surface floats above the ground, drawn as a shadow, named by role, lowest first. Each is an
/// equivalent CSS shadow; a shadow's radius here is half the CSS blur, since a SwiftUI shadow's radius
/// spreads about twice as far as the same CSS number, rounded to an even point.
enum Elevation: Sendable, CaseIterable {
  /// 0 2 8, ``Color/dropShadowSoft``: a surface resting on the ground, such as the live picture. An even blur
  /// radius keeps resting surfaces consistent while retaining a soft edge.
  case resting
  /// 0 14 28, ``Color/dropShadow``: a surface raised off the window's ground, such as a picture.
  case raised
  /// 0 14 30, ``Color/dropShadowStrong``: a surface over other content inside its own black ring, such as the mini
  /// screen over a pane; its 15-point radius rounded to 16.
  case overlay
  /// 0 16 40, ``Color/dropShadow``: a floating bar of buttons, as a menu floats.
  case floating
  /// 0 24 60, ``Color/dropShadow``: a modal surface over the scrim, such as an alert.
  case modal

  /// The shadow's colour.
  var color: Color {
    switch self {
    case .resting: .dropShadowSoft
    case .raised, .floating, .modal: .dropShadow
    case .overlay: .dropShadowStrong
    }
  }

  /// The shadow's blur radius: half the CSS blur.
  var radius: CGFloat {
    switch self {
    case .resting: 4
    case .raised: 14
    case .overlay: 16
    case .floating: 20
    case .modal: 30
    }
  }

  /// How far the shadow falls below the surface.
  var y: CGFloat {
    switch self {
    case .resting: 2
    case .raised, .overlay: 14
    case .floating: 16
    case .modal: 24
    }
  }
}
