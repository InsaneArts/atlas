/// The chip at the top of the screen.
package enum ViewerScreenChip: Equatable, Sendable {
  /// The input method is composing text not yet sent.
  ///
  /// - Parameter text: The text being composed.
  case typing(String)
  /// This window holds control.
  case inControl
  /// The picture is paused over the last one.
  case paused
}
