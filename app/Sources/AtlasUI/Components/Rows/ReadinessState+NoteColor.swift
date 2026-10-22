import SwiftUI

extension ReadinessState {
  /// The colour of what a check in this state means, under its name: the attention colour while it is not ready, a
  /// quiet one while it is unknown.
  var noteColor: Color {
    switch self {
    case .notReady: .textAttention
    case .ready, .unknown: .textTertiary
    }
  }
}
