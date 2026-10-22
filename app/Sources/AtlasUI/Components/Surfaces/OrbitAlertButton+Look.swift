import SwiftUI

extension OrbitAlertButton.Role {
  /// The capsule's look: blue for the default button, red for a destructive one, quiet for the way out.
  var look: CapsuleButtonStyle.Look {
    switch self {
    case .action, .safeCancel: .alertDefault
    case .cancel: .alertOther
    case .destructive: .alertDestructive
    }
  }

  /// The button's role for the system, which VoiceOver reads.
  var buttonRole: ButtonRole? {
    switch self {
    case .action: nil
    case .cancel, .safeCancel: .cancel
    case .destructive: .destructive
    }
  }

  /// The key that presses it: Return for the default button, Escape for the way out beside an action. A destructive
  /// alert's way out takes Return here and Escape from the alert (``OrbitAlertState/escape``).
  var shortcut: KeyboardShortcut? {
    switch self {
    case .action, .safeCancel: .defaultAction
    case .cancel: .cancelAction
    case .destructive: nil
    }
  }
}
