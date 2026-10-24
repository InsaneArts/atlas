/// One project Atlas taught, with Forget.
public struct SettingsProjectRow: Equatable, Sendable, Identifiable {
  /// The project's folder: what Forget sends.
  public var id: String
  /// The folder's name, such as "opie".
  public var name: String
  /// Where it is, such as "~/dev/opie".
  public var detail: String
  /// Forget's name for VoiceOver, such as "Forget opie".
  public var forgetLabel: String

  /// Creates a project's row.
  ///
  /// - Parameters:
  ///   - id: The project's folder: what Forget sends.
  ///   - name: The folder's name.
  ///   - detail: Where it is.
  ///   - forgetLabel: Forget's name for VoiceOver.
  public init(id: String, name: String, detail: String, forgetLabel: String) {
    self.id = id
    self.name = name
    self.detail = detail
    self.forgetLabel = forgetLabel
  }
}
