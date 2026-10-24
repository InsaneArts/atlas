/// The General pane: opening at login, the picture, and when Atlas tells the person it needs them. Its values are
/// edited through a binding; the controller writes each change where it belongs.
public struct SettingsGeneralState: Equatable, Sendable {
  /// Whether Atlas opens at login.
  public var opensAtLogin: Bool
  /// Whether this build can open at login at all; a development build cannot.
  public var canOpenAtLogin: Bool
  /// How often the windows show the helper's screen.
  public var picture: SettingsPicture
  /// The picture's choices, in menu order, each with whether it can be chosen now.
  public var pictureOptions: [PopupOption<SettingsPicture>]
  /// Whether Atlas notifies the person when it needs them.
  public var notify: Bool
  /// Whether Atlas badges its Dock icon when it needs the person.
  public var badge: Bool

  /// Creates the pane's state.
  ///
  /// - Parameters:
  ///   - opensAtLogin: Whether Atlas opens at login.
  ///   - canOpenAtLogin: Whether this build can open at login at all.
  ///   - picture: How often the windows show the helper's screen.
  ///   - pictureOptions: The picture's choices, in menu order.
  ///   - notify: Whether Atlas notifies the person when it needs them.
  ///   - badge: Whether Atlas badges its Dock icon when it needs the person.
  public init(
    opensAtLogin: Bool,
    canOpenAtLogin: Bool,
    picture: SettingsPicture,
    pictureOptions: [PopupOption<SettingsPicture>],
    notify: Bool,
    badge: Bool
  ) {
    self.opensAtLogin = opensAtLogin
    self.canOpenAtLogin = canOpenAtLogin
    self.picture = picture
    self.pictureOptions = pictureOptions
    self.notify = notify
    self.badge = badge
  }
}
