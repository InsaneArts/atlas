import SwiftUI

/// A moment's header as a button: a true capsule reaching past the column on each side, filled while the moment is
/// folded and clear while it is open; a step brighter under the pointer, and brighter again while held. The fill
/// fades as the moment opens or folds; a press shows at once and eases back. A moment held open has no capsule.
struct ActivityMomentStyle: ButtonStyle {
  /// Whether the moment is open.
  let isOpen: Bool
  /// Whether the person can fold or open it.
  let canFold: Bool
  @Environment(\.hoverPaused) private var hoverPaused
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// The header on its capsule.
  func makeBody(configuration: Configuration) -> some View {
    ActivityMomentCapsule(
      configuration: configuration,
      isOpen: isOpen,
      canFold: canFold,
      hoverPaused: hoverPaused,
      reduceMotion: reduceMotion
    )
  }
}
