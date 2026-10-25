import SwiftUI

extension AlertCardButton.Role {
  /// The capsule's look: blue for the default button, red for a destructive one, quiet for the way out.
  var look: CapsuleButtonStyle.Look {
    switch self {
    case .action, .safeCancel: .accent
    case .cancel: .tinted
    case .destructive: .destructive
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

  /// The spinner's arc while this button's command runs: the button's own word colour, white on the accent and red on
  /// a destructive button.
  var spinnerArc: Color {
    switch self {
    case .action, .safeCancel: .onAccent
    case .destructive: .textDestructive
    case .cancel: .textPrimary
    }
  }

  /// The key that presses it: Return for the default button, Escape for the way out beside an action. A destructive
  /// alert's way out takes Return here and Escape from the alert (``AlertCardState/escape``).
  var shortcut: KeyboardShortcut? {
    switch self {
    case .action, .safeCancel: .defaultAction
    case .cancel: .cancelAction
    case .destructive: nil
    }
  }
}
