/// The corner of the stage the small screen keeps to over Activity and Apps. The person can drag it anywhere, or throw
/// it; let go, it settles in the corner it was headed for, which the window remembers. The raw value is saved as the
/// person's choice and never changes.
public enum MiniCorner: String, CaseIterable, Sendable {
  /// Top left.
  case topLeading
  /// Top right.
  case topTrailing
  /// Bottom left.
  case bottomLeading
  /// Bottom right, where it starts.
  case bottomTrailing
}
