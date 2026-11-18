/// Whether a VM's window is open, as its runner sees it now: an open one
/// may take the person's input.
public enum WindowState: String, Codable, Equatable, Sendable {
  /// It is open.
  case open
  /// It is closed.
  case closed

  /// A window seen `visible` or `minimized`: open while it is either, as a
  /// minimized one is restored with its input and no change of its runner's;
  /// closed only once it is closed, its input off.
  public init(visible: Bool, minimized: Bool) {
    self = visible || minimized ? .open : .closed
  }
}
