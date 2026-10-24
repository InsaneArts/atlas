import SwiftUI

/// How far a surface floats above the ground, drawn as a shadow. Each is the boards' CSS shadow; a shadow's radius here
/// is half the CSS blur, since a SwiftUI shadow's radius spreads about twice as far as the same CSS number, rounded to
/// an even point.
enum Elevation: Sendable, CaseIterable {
  /// 0 14 28, black at 0.5: a picture lifted off the window's ground.
  case stage
  /// 0 14 30, black at 0.55: the mini screen over a pane, inside its own black ring; its 15-point radius rounded to 16.
  case mini
  /// 0 16 40, black at 0.5: a floating bar of buttons, as a menu floats (mac.py `MENU_SHADOW`).
  case bar
  /// 0 24 60, black at 0.55: an alert over the scrim.
  case alert

  /// The shadow's colour.
  var color: Color {
    switch self {
    case .stage, .bar: .black.opacity(0.5)
    case .mini, .alert: .black.opacity(0.55)
    }
  }

  /// The shadow's blur radius: half the CSS blur.
  var radius: CGFloat {
    switch self {
    case .stage: 14
    case .mini: 16
    case .bar: 20
    case .alert: 30
    }
  }

  /// How far the shadow falls below the surface.
  var y: CGFloat {
    switch self {
    case .stage, .mini: 14
    case .bar: 16
    case .alert: 24
    }
  }
}
