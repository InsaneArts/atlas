import SwiftUI

/// What opened the picture sheet takes the keyboard and VoiceOver back as the sheet closes, by Done, Escape or a
/// press on the dimmed window: the picture or the step's picture button whose picture the sheet showed last.
struct SheetOpenerFocus: ViewModifier {
  /// Whether the sheet shows this view's picture now.
  let isCurrent: Bool
  @Environment(\.pictureSheetShown) private var sheetShown
  /// Whether the sheet showed this view's picture last, so it takes focus back as the sheet closes.
  @State private var showedLast = false
  @FocusState private var focused: Bool
  @AccessibilityFocusState private var spoken: Bool

  /// The view, focused again as the sheet closes when it showed its picture last.
  ///
  /// - Parameter content: The opener.
  /// - Returns: The opener.
  func body(content: Content) -> some View {
    content
      .focused($focused)
      .accessibilityFocused($spoken)
      .onChange(of: isCurrent, initial: true) { _, current in
        // Stepping to another picture in the sheet moves the place to return to with it.
        if current {
          showedLast = true
        } else if sheetShown {
          showedLast = false
        }
      }
      .onChange(of: sheetShown) { _, shown in
        guard !shown, showedLast else { return }
        showedLast = false
        focused = true
        spoken = true
      }
  }
}
