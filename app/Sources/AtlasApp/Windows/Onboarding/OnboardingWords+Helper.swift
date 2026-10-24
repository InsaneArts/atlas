import AtlasOnboarding

/// The helper's side of setup: allowing the account that asked, each permission and its guide beside System
/// Settings, and the helper's last screen.
extension OnboardingWords {
  /// A permission that is on.
  static let permissionOn = "Allowed"
  /// A permission that is off.
  static let permissionOff = "Off"
  /// A permission not read yet: Atlas's job did not answer.
  static let permissionUnknown = "Checking…"
  /// The helper's done tail when no account asked.
  static let switchBackToOwn = "Switch back to your own account."

  /// The allow screen's label over its title.
  ///
  /// - Parameter account: This account as people know it.
  /// - Returns: "Atlas in the Atlas account"
  static func atlasIn(_ account: String) -> String {
    "Atlas in \(named(account))"
  }

  /// The allow screen's headline.
  ///
  /// - Parameter asker: The asking account as people know it.
  /// - Returns: "Let Ismayil use this account"
  static func letUse(_ asker: String) -> String {
    "Let \(named(asker)) use this account"
  }

  /// What allowing means.
  ///
  /// - Parameter asker: The asking account as people know it.
  /// - Returns: The allow screen's lede.
  static func allowLede(_ asker: String) -> String {
    "\(asker)'s agents will open apps here, click and type, while \(asker) keeps working in their own account. You allow this once; Atlas then starts here by itself."
  }

  /// The button that opens a permission's list in System Settings.
  static let allowButton = "Allow…"

  /// What to do in System Settings: drag Atlas into the list; for Screen Recording, the top one of its two lists
  /// (the lower one grants sound only).
  ///
  /// - Parameter permission: The permission allowed now.
  /// - Returns: "Drag Atlas into the list."
  static func dragInto(_ permission: Permission) -> String {
    switch permission {
    case .accessibility: "Drag Atlas into the list."
    case .screenRecording: "Drag Atlas into the top list."
    }
  }

  /// The panel's draggable icon, as VoiceOver names it.
  static let atlas = "Atlas"
  /// The panel's icon's tooltip.
  static let dragHelp = "Drag Atlas into the list in System Settings"

  /// For Atlas listed already, switched off (dragging it in again may leave it off), and for anyone who can't drag.
  ///
  /// - Parameter permission: The permission allowed now.
  /// - Returns: "Already in the list? Turn its switch on."
  static func alreadyListed(_ permission: Permission) -> String {
    switch permission {
    case .accessibility: "Already in the list? Turn its switch on."
    case .screenRecording: "Already in the top list? Turn its switch on."
    }
  }

  /// The panel's label over its icon.
  ///
  /// - Parameters:
  ///   - number: Which permission, from 1.
  ///   - count: How many.
  ///   - permission: Its name.
  /// - Returns: "1 of 2 · Accessibility"
  static func panelLabel(_ number: Int, of count: Int, _ permission: String) -> String {
    "\(number) of \(count) · \(permission)"
  }

  /// The panel's pill while it waits.
  static let waitingForSwitch = "Waiting for the switch…"

  /// The helper's done tail.
  ///
  /// - Parameter asker: The account to switch back to, as people know it.
  /// - Returns: "Switch back to Ismayil."
  static func switchBack(to asker: String) -> String {
    "Switch back to \(named(asker))."
  }
}
