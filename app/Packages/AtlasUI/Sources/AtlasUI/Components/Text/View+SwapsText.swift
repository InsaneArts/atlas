import SwiftUI

extension View {
  /// Swaps the view's words in place by opacity when `value` changes, over ``Motion/textSwap``.
  ///
  /// - Parameter value: What changes, such as the words themselves.
  /// - Returns: The view, crossfading its words.
  func swapsText<Value: Equatable>(on value: Value) -> some View {
    modifier(TextSwap(value: value))
  }
}
