extension Array {
  /// The word of the choice `value` names, as a popup's button shows it.
  ///
  /// - Parameter value: The current choice.
  /// - Returns: Its word, or empty when no choice is it.
  func title<Value: Hashable & Sendable>(of value: Value) -> String where Element == PopupOption<Value> {
    first { $0.id == value }?
      .title ?? ""
  }
}
