import SwiftUI

extension StatusTone {
  /// The mark's paint: green when ready, blue while working or in control, orange when it needs the person, grey
  /// otherwise.
  var color: Color {
    switch self {
    case .ready: .success
    case .active, .link: .accent
    case .attention: .attention
    case .quiet: .textTertiary
    }
  }

  /// The word's paint: the link colour in control, the attention colour when it needs the person, plain otherwise.
  var wordColor: Color {
    switch self {
    case .link: .textLink
    case .attention: .textAttention
    case .ready, .active, .quiet: .textPrimary
    }
  }

  /// Whether a dot in this tone glows: every colour but the quiet grey.
  var glows: Bool {
    self != .quiet
  }
}
