import AppKit
import AtlasControl

extension NSEvent {
  /// The modifier keys held, in Atlas's own terms: Command, Control, Option and Shift.
  var heldModifiers: Modifiers {
    let flags = modifierFlags
    var modifiers: Modifiers = []
    if flags.contains(.command) { modifiers.insert(.command) }
    if flags.contains(.control) { modifiers.insert(.control) }
    if flags.contains(.option) { modifiers.insert(.option) }
    if flags.contains(.shift) { modifiers.insert(.shift) }
    return modifiers
  }
}
