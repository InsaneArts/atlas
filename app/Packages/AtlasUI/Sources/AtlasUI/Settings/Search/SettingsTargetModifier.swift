import SwiftUI

/// An inert focus stop around the original row, so revealing a command cannot execute it.
private struct SettingsTargetModifier: ViewModifier {
  /// The row or section's stable identity.
  let target: SettingsSearchTarget
  /// The latest explicit reveal.
  @Environment(\.settingsReveal) private var reveal
  /// Focus belongs to the inert wrapper, never to its action button.
  @FocusState private var focused: Bool

  /// Identifies the scrolling anchor and draws a quiet focus outline on explicit opens.
  ///
  /// - Parameter content: The original row.
  /// - Returns: The row with inert target focus.
  func body(content: Content) -> some View {
    content
      .id(target)
      .focusable()
      .focused($focused)
      .focusEffectDisabled()
      .onKeyPress(.return) { focused ? .handled : .ignored }
      .overlay {
        RoundedRectangle(cornerRadius: Radius.small)
          .strokeBorder(focused ? Color.strokeFocus : Color.clear, lineWidth: Stroke.regular)
          .allowsHitTesting(false)
      }
      .onChange(of: reveal) { _, request in
        if request?.target == target { focused = true }
      }
  }
}

extension View {
  /// Gives search a scroll anchor and a safe focus destination around the existing control.
  ///
  /// - Parameter target: The row's stable identity.
  /// - Returns: The anchored row.
  func settingsTarget(_ target: SettingsSearchTarget) -> some View {
    modifier(SettingsTargetModifier(target: target))
  }
}
