/// Why the deletion-only entry deleted nothing.
public enum PasswordForgettingError: Error, Equatable, CustomStringConvertible {
  /// No removal of the VM is recorded, or its record cannot be read.
  case noRemoval
  /// The removal recorded is another one.
  case anotherRemoval
  /// The removal has not come to its passwords: a run of the VM may live.
  case notYet
  /// What became of the passwords could not be kept.
  case unkept

  /// What went wrong, as the person reads it.
  public var description: String {
    switch self {
    case .noRemoval: "No removal of this VM is recorded; nothing was deleted."
    case .anotherRemoval: "Another removal of this VM is recorded; nothing was deleted."
    case .notYet: "This VM's removal has not come to its passwords; nothing was deleted."
    case .unkept: "What became of the VM's passwords could not be kept."
    }
  }
}
