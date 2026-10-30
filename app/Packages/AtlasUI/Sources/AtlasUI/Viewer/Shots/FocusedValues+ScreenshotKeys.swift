import SwiftUI

extension FocusedValues {
  /// Whether the keyboard focus belongs to screenshot content.
  var screenshotKeys: Bool? {
    get { self[ScreenshotFocusKey.self] }
    set { self[ScreenshotFocusKey.self] = newValue }
  }
}
