/// The Helper pane's "Your files" section: whether the home folder is private from the helper, whether agents may copy
/// sign-ins in, and what the last change said.
public struct SettingsFilesState: Equatable, Sendable {
  /// The home folder's row; nil while Atlas can't say how it stands.
  public var home: SettingsFilesRow?
  /// Whether agents may copy sign-ins and secrets into the helper; nil while Atlas can't say.
  public var signInCopies: Bool?
  /// Whether the sign-ins switch takes changes now; it waits while a change runs.
  public var canChangeSignIns: Bool
  /// What the last change said, or that one runs; nil for nothing.
  public var status: SettingsNote?

  /// Creates the section.
  ///
  /// - Parameters:
  ///   - home: The home folder's row; nil while unknown.
  ///   - signInCopies: Whether agents may copy sign-ins in; nil while unknown.
  ///   - canChangeSignIns: Whether the sign-ins switch takes changes now.
  ///   - status: What the last change said; nil by default.
  public init(home: SettingsFilesRow?, signInCopies: Bool?, canChangeSignIns: Bool, status: SettingsNote? = nil) {
    self.home = home
    self.signInCopies = signInCopies
    self.canChangeSignIns = canChangeSignIns
    self.status = status
  }
}
