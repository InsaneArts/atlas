/// The chip at the top of the screen.
public enum ViewerScreenChip: Equatable, Sendable {
  /// The input method is composing text not yet sent.
  ///
  /// - Parameter text: The text being composed.
  case typing(String)
  /// This window holds control.
  case inControl
  /// The picture is paused over the last one.
  case paused
  /// A host session has the helper, while two or more are live.
  ///
  /// - Parameters:
  ///   - name: The session's name, such as "Claude Code · understudy".
  ///   - icon: The host's icon.
  case holder(name: String, icon: RowIcon)
}
