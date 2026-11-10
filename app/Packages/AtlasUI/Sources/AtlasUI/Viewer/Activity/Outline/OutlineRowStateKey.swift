import SwiftUI

/// How a boxed row tells its box it is pressed or focused.
struct OutlineRowStateKey: PreferenceKey {
  /// Neither pressed nor focused.
  static let defaultValue = OutlineRowState()

  /// The row's own state; a box holds one row.
  ///
  /// - Parameters:
  ///   - value: The state so far.
  ///   - nextValue: The next row's state.
  static func reduce(value: inout OutlineRowState, nextValue: () -> OutlineRowState) {
    let next = nextValue()
    value = OutlineRowState(
      isPressed: value.isPressed || next.isPressed,
      isFocused: value.isFocused || next.isFocused
    )
  }
}
