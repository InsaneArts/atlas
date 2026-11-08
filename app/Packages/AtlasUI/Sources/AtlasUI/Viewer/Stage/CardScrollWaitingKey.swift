import SwiftUI

/// How a ``CardScroll``'s content tells it what waits above a person scrolled down, such as "3 new sessions".
struct CardScrollWaitingKey: PreferenceKey {
  /// Nothing waits.
  static let defaultValue: String? = nil

  /// The first words given; a list has one line.
  ///
  /// - Parameters:
  ///   - value: The words so far.
  ///   - nextValue: The next view's words.
  static func reduce(value: inout String?, nextValue: () -> String?) {
    value = value ?? nextValue()
  }
}
