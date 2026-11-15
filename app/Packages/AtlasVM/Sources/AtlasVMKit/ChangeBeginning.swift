/// What asking for a change finds.
public enum ChangeBeginning: Equatable, Sendable {
  /// It is new, and is now kept as intended: it may be done.
  case new
  /// The same change was asked before: this is its record, intended or
  /// settled, and it is never done again.
  case asked(ChangeRecord)
  /// Another change was asked with its ID: refused, never done.
  case conflicting
}
