/// The General pane: opening at login, the picture, muting the helper, and when Atlas tells the person it needs them.
/// Its values are edited through a binding; the controller writes each change where it belongs.
public struct SettingsGeneralState: Equatable, Sendable {
  /// Whether Atlas opens at login.
  public var opensAtLogin: Bool
  /// Whether this build can open at login at all; a development build cannot.
  public var canOpenAtLogin: Bool
  /// How often the windows show the helper's screen.
  public var picture: SettingsPicture
  /// The picture's choices, in menu order, each with whether it can be chosen now.
  public var pictureOptions: [PopupOption<SettingsPicture>]
  /// Whether apps in the helper stay silent on the person's speakers.
  public var muteHelper: Bool
  /// Why the helper is not muted though the person wants it, with the one step that fixes it; nil when it is fine.
  public var soundProblem: String?
  /// Whether Atlas notifies the person when it needs them.
  public var notify: Bool

  /// Creates the pane's state.
  ///
  /// - Parameters:
  ///   - opensAtLogin: Whether Atlas opens at login.
  ///   - canOpenAtLogin: Whether this build can open at login at all.
  ///   - picture: How often the windows show the helper's screen.
  ///   - pictureOptions: The picture's choices, in menu order.
  ///   - muteHelper: Whether apps in the helper stay silent on the person's speakers.
  ///   - soundProblem: Why the helper is not muted though the person wants it, and its fix; nil when it is fine.
  ///   - notify: Whether Atlas notifies the person when it needs them.
  public init(
    opensAtLogin: Bool,
    canOpenAtLogin: Bool,
    picture: SettingsPicture,
    pictureOptions: [PopupOption<SettingsPicture>],
    muteHelper: Bool,
    soundProblem: String?,
    notify: Bool
  ) {
    self.opensAtLogin = opensAtLogin
    self.canOpenAtLogin = canOpenAtLogin
    self.picture = picture
    self.pictureOptions = pictureOptions
    self.muteHelper = muteHelper
    self.soundProblem = soundProblem
    self.notify = notify
  }
}
