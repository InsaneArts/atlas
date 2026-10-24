import SwiftUI

/// Swaps changing words in place: the old words fade as the new ones come, over ``Motion/textSwap``, with
/// nothing blurring, sliding or rolling, as the system updates its own status text. Only opacity changes, which
/// Reduce Motion keeps, so it reads the same with Reduce Motion on. Words that change more than once a second, such
/// as a status line while an agent works, take no transition at all instead.
struct TextSwap<Value: Equatable>: ViewModifier {
  /// What changes, such as the words themselves.
  let value: Value

  /// The words, crossfading when `value` changes.
  ///
  /// - Parameter content: The words.
  /// - Returns: The words, swapping by opacity.
  func body(content: Content) -> some View {
    content
      .contentTransition(.opacity)
      .animation(Motion.textSwap, value: value)
  }
}
