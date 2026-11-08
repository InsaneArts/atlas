import SwiftUI

extension View {
  /// Takes the keyboard and VoiceOver back as the picture sheet closes, when the sheet showed this view's picture
  /// last (``SheetOpenerFocus``).
  ///
  /// - Parameters:
  ///   - isCurrent: Whether the sheet shows this view's picture now.
  ///   - keys: Whether the keyboard comes back to it; it does by default.
  ///   - voiceOver: Whether VoiceOver comes back to it; it does by default.
  /// - Returns: The view.
  func takesFocusAfterSheet(isCurrent: Bool, keys: Bool = true, voiceOver: Bool = true) -> some View {
    modifier(SheetOpenerFocus(isCurrent: isCurrent, keys: keys, voiceOver: voiceOver))
  }
}
