/// Whether a VM's window is open, as its runner sees it now: an open one
/// may take the person's input.
public enum WindowState: String, Codable, Equatable, Sendable {
  /// It is open.
  case open
  /// It is closed.
  case closed
}
