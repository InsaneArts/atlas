import SwiftUI

/// How far a surface floats above the ground, drawn as a shadow.
enum Elevation: Sendable {
  /// A picture lifted off the window's ground.
  case stage

  /// The shadow's colour.
  var color: Color {
    switch self {
    case .stage: .black.opacity(0.5)
    }
  }

  /// The shadow's blur radius.
  var radius: CGFloat {
    switch self {
    case .stage: 28
    }
  }

  /// How far the shadow falls below the surface.
  var y: CGFloat {
    switch self {
    case .stage: 14
    }
  }
}
