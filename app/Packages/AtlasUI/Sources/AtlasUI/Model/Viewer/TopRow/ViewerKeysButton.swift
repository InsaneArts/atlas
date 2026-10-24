/// The top row's Keys button (v3 D11): shown only while this window holds control, since only then has it keys to send,
/// and pressed while its menu is open.
public enum ViewerKeysButton: Equatable, Sendable {
  /// Not shown: this window holds no control.
  case hidden
  /// Shown, with whether its menu is open.
  ///
  /// - Parameter isOpen: Whether its menu is open, which draws it pressed.
  case shown(isOpen: Bool)
}
