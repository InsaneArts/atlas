/// The General pane: opening at login, the picture and muting the helper; pictures of agent actions, how long they are
/// kept, how much typed text is kept and how many this Mac holds; and when Atlas tells the person it needs them. Its
/// values are edited through a binding; the controller writes each change where it belongs.
public struct SettingsGeneralState: Equatable, Sendable {
  /// 1 to 365: the days a custom choice keeps pictures.
  public static let keepDaysRange = 1...365
  /// 1 or more: the characters a limit on typed text keeps, as Atlas's settings take any whole number from 1.
  public static let typedTextRange = 1...Int(UInt32.max)
  /// 10: how far the typed text's limit steps.
  public static let typedTextStep = 10
  /// 80: the characters Limited… keeps until the person changes it.
  public static let typedTextDefault = 80

  /// Whether Atlas opens at login.
  public var opensAtLogin: Bool
  /// Whether this build can open at login at all; a development build cannot.
  public var canOpenAtLogin: Bool
  /// The glyph Atlas's menu bar item draws.
  public var menuBarIcon: MenuBarIcon
  /// How often the windows show the helper's screen.
  public var picture: SettingsPicture
  /// The picture's choices, in menu order, each with whether it can be chosen now.
  public var pictureOptions: [PopupOption<SettingsPicture>]
  /// Whether apps in the helper stay silent on the person's speakers.
  public var muteHelper: Bool
  /// Why the helper is not muted though the person wants it, with the one step that fixes it; nil when it is fine.
  public var soundProblem: String?
  /// Whether Atlas keeps pictures of the helper's screen around each agent action.
  public var picturesBeforeActions: Bool
  /// How long pictures are kept.
  public var keepFor: SettingsKeepFor
  /// The days a custom choice keeps pictures.
  public var keepDays: Int
  /// How much typed text is kept.
  public var typedText: SettingsTypedText
  /// The characters a limit keeps.
  public var typedTextLimit: Int
  /// How many pictures this Mac holds and their size, such as "1,240 pictures · 142 MB"; empty while unknown.
  public var storedPictures: String
  /// Whether there are pictures to delete.
  public var canDeletePictures: Bool
  /// Whether Atlas notifies the person when it needs them.
  public var notify: Bool
  /// What the last change of the pane said went wrong, under its pictures; nil when nothing did.
  public var notice: SettingsNote?

  /// Creates the pane's state.
  ///
  /// - Parameters:
  ///   - opensAtLogin: Whether Atlas opens at login.
  ///   - canOpenAtLogin: Whether this build can open at login at all.
  ///   - menuBarIcon: The glyph Atlas's menu bar item draws; the Orbit by default.
  ///   - picture: How often the windows show the helper's screen.
  ///   - pictureOptions: The picture's choices, in menu order.
  ///   - muteHelper: Whether apps in the helper stay silent on the person's speakers.
  ///   - soundProblem: Why the helper is not muted though the person wants it, and its fix; nil when it is fine.
  ///   - picturesBeforeActions: Whether pictures are kept around each agent action; on by default.
  ///   - keepFor: How long pictures are kept; seven days by default.
  ///   - keepDays: The days a custom choice keeps pictures; seven by default.
  ///   - typedText: How much typed text is kept; all of it by default.
  ///   - typedTextLimit: The characters a limit keeps; 80 by default.
  ///   - storedPictures: How many pictures this Mac holds and their size; unknown by default.
  ///   - canDeletePictures: Whether there are pictures to delete; there are not by default.
  ///   - notify: Whether Atlas notifies the person when it needs them.
  ///   - notice: What the last change said went wrong; none by default.
  public init(
    opensAtLogin: Bool,
    canOpenAtLogin: Bool,
    menuBarIcon: MenuBarIcon = .orbit,
    picture: SettingsPicture,
    pictureOptions: [PopupOption<SettingsPicture>],
    muteHelper: Bool,
    soundProblem: String?,
    picturesBeforeActions: Bool = true,
    keepFor: SettingsKeepFor = .sevenDays,
    keepDays: Int = 7,
    typedText: SettingsTypedText = .full,
    typedTextLimit: Int = typedTextDefault,
    storedPictures: String = "",
    canDeletePictures: Bool = false,
    notify: Bool,
    notice: SettingsNote? = nil
  ) {
    self.opensAtLogin = opensAtLogin
    self.canOpenAtLogin = canOpenAtLogin
    self.menuBarIcon = menuBarIcon
    self.picture = picture
    self.pictureOptions = pictureOptions
    self.muteHelper = muteHelper
    self.soundProblem = soundProblem
    self.picturesBeforeActions = picturesBeforeActions
    self.keepFor = keepFor
    self.keepDays = keepDays
    self.typedText = typedText
    self.typedTextLimit = typedTextLimit
    self.storedPictures = storedPictures
    self.canDeletePictures = canDeletePictures
    self.notify = notify
    self.notice = notice
  }
}
