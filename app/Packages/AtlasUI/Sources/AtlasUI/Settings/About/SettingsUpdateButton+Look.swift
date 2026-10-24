import SwiftUI

extension SettingsUpdateButton {
  /// The button's word.
  var title: String {
    switch self {
    case .check: "Check for Updates"
    case .checking: "Checking…"
    case .install: "Install Update…"
    case .installing: "Installing…"
    }
  }

  /// How it is painted: white to check, blue once a newer version waits, the one time About asks for something.
  var look: CapsuleButtonStyle.Look {
    switch self {
    case .check, .checking: .primary
    case .install, .installing: .accent
    }
  }

  /// Whether its check or install runs now, which shows a spinner in it and holds it.
  var isWorking: Bool {
    switch self {
    case .checking, .installing: true
    case .check, .install: false
    }
  }

  /// The spinner's arc: the ground's colour on white, white on the accent.
  var arc: Color {
    look == .accent ? .onAccent : .ground
  }

  /// What a press sends, or nil while it works.
  var event: SettingsEvent? {
    switch self {
    case .check: .checkForUpdates
    case .install: .askInstall
    case .checking, .installing: nil
    }
  }
}
