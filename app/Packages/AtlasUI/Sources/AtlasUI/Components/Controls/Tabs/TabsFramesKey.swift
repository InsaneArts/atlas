import SwiftUI

/// The padded label frames, keyed by their index in the shared track.
struct TabsFramesKey: PreferenceKey {
  /// An unmeasured track.
  static let defaultValue: [Int: CGRect] = [:]

  /// Combines the labels' frames without allowing one sibling to erase another.
  ///
  /// - Parameters:
  ///   - value: The frames already measured.
  ///   - nextValue: The next label's frame.
  static func reduce(value: inout [Int: CGRect], nextValue: () -> [Int: CGRect]) {
    value.merge(nextValue(), uniquingKeysWith: { _, new in new })
  }
}
