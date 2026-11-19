/// The Desktops page, shown in Helper's place once more than one desktop is set up: each desktop, opening its own
/// page, Add a Desktop… under them, and what the last change said.
public struct SettingsDesktopsState: Equatable, Sendable {
  /// Every desktop, in the settings' order.
  public var rows: [SettingsDesktopRow]
  /// What the last change said, under the list; nil for nothing.
  public var notice: SettingsNote?

  /// Creates the page's state.
  ///
  /// - Parameters:
  ///   - rows: Every desktop, in the settings' order.
  ///   - notice: What the last change said; none by default.
  public init(rows: [SettingsDesktopRow], notice: SettingsNote? = nil) {
    self.rows = rows
    self.notice = notice
  }
}
