/// What became of one of a VM's passwords when its removal asked for it to
/// be deleted.
public enum PasswordFate: String, Codable, Equatable, Sendable {
  /// It was there, and is deleted.
  case deleted
  /// There was none.
  case absent
  /// It could not be deleted without asking, and is still there.
  case left
}
