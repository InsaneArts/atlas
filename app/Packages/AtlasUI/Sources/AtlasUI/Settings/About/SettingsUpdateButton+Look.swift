import SwiftUI

extension SettingsUpdateButton {
  /// The button's word: Check for Updates until a newer version is found, then Install Update…. It keeps its word
  /// while its check or install runs; the line under it says so.
  var title: String {
    switch self {
    case .check, .checking: "Check for Updates"
    case .install, .installing: "Install Update…"
    }
  }

  /// Every word the button can show, so it lays out at the widest and never changes width.
  static let titles = ["Check for Updates", "Install Update…"]

  /// How it is painted: glass to check, blue once a newer version waits, the one time About asks for something.
  var look: CapsuleButtonStyle.Look {
    switch self {
    case .check, .checking: .glass
    case .install, .installing: .accent
    }
  }

  /// Whether its check or install runs now, which holds it.
  var isWorking: Bool {
    switch self {
    case .checking, .installing: true
    case .check, .install: false
    }
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
