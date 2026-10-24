/// One project Atlas taught, with Forget.
public struct SettingsProjectRow: Equatable, Sendable, Identifiable {
  /// The project's folder: what Forget sends.
  public var id: String
  /// The folder's name, such as "opie".
  public var name: String
  /// Where it is, such as "~/dev/opie", or that its files no longer have the Atlas section.
  public var detail: String
  /// Whether the detail is the folder's path, set monospaced (v3 G8).
  public var detailIsPath: Bool
  /// Forget's name for VoiceOver, such as "Forget opie".
  public var forgetLabel: String

  /// Creates a project's row.
  ///
  /// - Parameters:
  ///   - id: The project's folder: what Forget sends.
  ///   - name: The folder's name.
  ///   - detail: Where it is.
  ///   - detailIsPath: Whether the detail is the folder's path.
  ///   - forgetLabel: Forget's name for VoiceOver.
  public init(id: String, name: String, detail: String, detailIsPath: Bool = true, forgetLabel: String) {
    self.id = id
    self.name = name
    self.detail = detail
    self.detailIsPath = detailIsPath
    self.forgetLabel = forgetLabel
  }
}
