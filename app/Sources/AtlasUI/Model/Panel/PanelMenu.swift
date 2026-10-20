/// The menu rows under the panel's cards. Getting Started… and Quit Atlas always show.
package struct PanelMenu: Equatable, Sendable {
  /// Open at Login's switch, on or off; nil hides the row, when the app cannot be a login item.
  package var opensAtLogin: Bool?
  /// Whether Recover shows, and whether it can be chosen.
  package var recover: PanelMenuItem
  /// Whether Log Out Helper… shows, and whether it can be chosen.
  package var logOut: PanelMenuItem

  /// Creates the menu rows.
  ///
  /// - Parameters:
  ///   - opensAtLogin: Open at Login's switch, on or off; nil hides the row, when the app cannot be a login item.
  ///   - recover: Whether Recover shows, and whether it can be chosen.
  ///   - logOut: Whether Log Out Helper… shows, and whether it can be chosen.
  package init(opensAtLogin: Bool? = nil, recover: PanelMenuItem, logOut: PanelMenuItem) {
    self.opensAtLogin = opensAtLogin
    self.recover = recover
    self.logOut = logOut
  }
}
