import SwiftUI

/// How text is painted, by what it says: a title, a detail, a remark, or something that needs the person.
enum TextTone: Sendable {
  /// Titles and values.
  case primary
  /// Details and labels.
  case secondary
  /// Remarks, small print and names of sections.
  case quiet
  /// Something needs the person.
  case attention

  /// The tone's paint.
  var color: Color {
    switch self {
    case .primary: .textPrimary
    case .secondary: .textSecondary
    case .quiet: .textTertiary
    case .attention: .attention
    }
  }
}
