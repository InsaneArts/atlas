import SwiftUI

/// Back and forward as one raised capsule, two halves either side of a hairline, as a browser's toolbar has them: a
/// half with nowhere to go stands in tertiary and takes no press. ⌘[ and ⌘] press them from the keyboard.
struct NavCapsule: View {
  /// Whether Back has somewhere to go.
  let canGoBack: Bool
  /// Whether Forward has somewhere to go.
  let canGoForward: Bool
  /// Receives Back or Forward.
  let send: @MainActor (SettingsEvent) -> Void

  /// The two halves on their capsule.
  var body: some View {
    HStack(spacing: .zero) {
      half(.back, label: "Back", isEnabled: canGoBack, key: "[", event: .goBack)
      Rectangle()
        .fill(Color.separator)
        .frame(width: Stroke.regular, height: SettingsLayout.navDivider)
      half(.next, label: "Forward", isEnabled: canGoForward, key: "]", event: .goForward)
    }
    .background { PillSelection() }
  }

  /// One half: its chevron, pressed by the pointer or its shortcut while it has somewhere to go.
  ///
  /// - Parameters:
  ///   - symbol: Its chevron.
  ///   - label: Its name, read by VoiceOver and shown as its tooltip.
  ///   - isEnabled: Whether it has somewhere to go.
  ///   - key: Its shortcut's key, with ⌘.
  ///   - event: What it sends.
  /// - Returns: The half.
  private func half(
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
        .font(Typography.font(.body, weight: .semibold))
        .foregroundStyle(isEnabled ? Color.textPrimary : Color.textTertiary)
        .frame(width: SettingsLayout.navHalf, height: SettingsLayout.navHeight)
    }
    .buttonStyle(.pill)
    .keyboardShortcut(key, modifiers: .command)
    .disabled(!isEnabled)
    .help(Text(verbatim: label))
    .accessibilityLabel(Text(verbatim: label))
  }
}
