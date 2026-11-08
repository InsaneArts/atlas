import SwiftUI

extension View {
  /// Takes the keyboard and VoiceOver back as the picture sheet closes, when the sheet showed this view's picture
  /// last (``SheetOpenerFocus``).
  ///
  /// - Parameter isCurrent: Whether the sheet shows this view's picture now.
  /// - Returns: The view.
  func takesFocusAfterSheet(isCurrent: Bool) -> some View {
    modifier(SheetOpenerFocus(isCurrent: isCurrent))
  }
}
