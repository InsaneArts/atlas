import CoreGraphics

/// Where System Settings' window is, as a test moves it.
@MainActor
final class SettingsFrame {
  /// The frame now.
  var frame: CGRect

  /// Creates the frame.
  ///
  /// - Parameter frame: Where it starts.
  init(_ frame: CGRect) {
    self.frame = frame
  }
}
