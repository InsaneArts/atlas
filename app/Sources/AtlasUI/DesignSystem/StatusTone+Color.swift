import SwiftUI

extension StatusTone {
  /// The tone's paint: green when ready, blue while working, orange when it needs the person, quiet grey otherwise.
  var color: Color {
    switch self {
    case .ready: .success
    case .active: .accent
    case .attention: .attention
    case .quiet: .textTertiary
    }
  }
}
