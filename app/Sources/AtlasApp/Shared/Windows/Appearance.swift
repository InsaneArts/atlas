import AppKit

/// The person's display settings that AppKit motion reads, read as each transition starts, so a change in System
/// Settings applies to the next one. SwiftUI views read the environment's `accessibilityReduceMotion` instead.
enum Appearance {
  /// Whether Reduce Motion is on: a window then moves at once instead of gliding.
  static var reduceMotion: Bool {
    NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
  }
}
