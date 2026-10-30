import SwiftUI

extension MonogramTint {
  /// The tint's paint, the same in either appearance, as an app's own icon is.
  var color: Color {
    switch self {
    case .indigo: .monogramIndigo
    case .blue: .monogramBlue
    case .teal: .monogramTeal
    case .green: .monogramGreen
    case .amber: .monogramAmber
    case .coral: .monogramCoral
    case .pink: .monogramPink
    case .graphite: .monogramGraphite
    }
  }
}
