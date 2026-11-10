import SwiftUI

/// Back and forward, as the main window's title-bar buttons stand: each its chevron alone in a circle, unfilled at rest,
/// lit under the pointer and held while pressed (``CapsuleButtonStyle/Look/glyph``); one with nowhere to go dims and
/// takes no press. ⌘[ and ⌘] press them from the keyboard.
struct NavButtons: View {
  /// Whether Back has somewhere to go.
  let canGoBack: Bool
  /// Whether Forward has somewhere to go.
  let canGoForward: Bool
  /// Receives Back or Forward.
  let send: @MainActor (SettingsEvent) -> Void

  /// The two buttons, side by side.
  var body: some View {
    HStack(spacing: Spacing.xs) {
      button(.back, label: "Back", isEnabled: canGoBack, key: "[", event: .goBack)
      button(.next, label: "Forward", isEnabled: canGoForward, key: "]", event: .goForward)
    }
  }

  /// One button: its chevron, pressed by the pointer or its shortcut while it has somewhere to go.
  ///
  /// - Parameters:
  ///   - symbol: Its chevron.
  ///   - label: Its name, read by VoiceOver and shown as its tooltip.
  ///   - isEnabled: Whether it has somewhere to go.
  ///   - key: Its shortcut's key, with ⌘.
  ///   - event: What it sends.
  /// - Returns: The button.
  private func button(
    _ symbol: SymbolName,
    label: String,
    isEnabled: Bool,
    key: KeyEquivalent,
    event: SettingsEvent
  ) -> some View {
    Button {
      send(event)
    } label: {
      Image(systemName: symbol.rawValue)
    }
    .buttonStyle(.capsule(.glyph, size: .compact, outline: .circle))
    .keyboardShortcut(key, modifiers: .command)
    .disabled(!isEnabled)
    .help(Text(verbatim: label))
    .accessibilityLabel(Text(verbatim: label))
  }
}
