import SwiftUI

/// Only the picture surface owns screenshot shortcuts. Its neighbouring buttons keep native activation keys.
struct ScreenshotFocus: ViewModifier {
  /// Whether this screenshot surface is interactive.
  var enabled = true
  /// A newly opened large screenshot takes focus; choosing a sidebar destination does not.
  var takesFocus = false
  /// Which picture surface this modifier represents.
  var target = ScreenshotFocusTarget.grid
  @FocusState private var localFocus: ScreenshotFocusTarget?
  @Environment(\.screenshotFocus) private var sharedFocus

  /// The screenshot subtree, taking focus when pressed and relinquishing it when hidden.
  func body(content: Content) -> some View {
    let focus = sharedFocus ?? $localFocus
    return
      content
      .focusable(enabled)
      .focusEffectDisabled()
      .focused(focus, equals: target)
      .focusedValue(\.screenshotKeys, focus.wrappedValue == target ? true : nil)
      .simultaneousGesture(TapGesture().onEnded { if enabled { focus.wrappedValue = target } })
      .onChange(of: enabled, initial: true) { _, enabled in
        if !enabled, focus.wrappedValue == target {
          focus.wrappedValue = nil
        } else if enabled, takesFocus {
          focus.wrappedValue = target
        }
      }
  }
}
