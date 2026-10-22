/// The top row's Keys button: pressed while this window holds control, faded while it does not, and gone while the
/// helper needs the person.
package enum ViewerKeysButton: Equatable, Sendable {
  /// Not shown.
  case hidden
  /// Shown, faded, and not pressable: there is no control to send keys with.
  case disabled
  /// Shown pressed, as control is on, and pressable: it offers the keys this Mac would keep.
  case pressed
}
