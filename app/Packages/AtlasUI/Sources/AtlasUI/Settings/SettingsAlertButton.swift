import SwiftUI

/// A button whose own focus binding survives an alert, independently of AppKit's shared hosting responder.
struct SettingsAlertButton<Label: View>: View {
  /// Whether the current action opens an alert, as opposed to checking for updates.
  var opensAlert = true
  /// The existing action.
  let action: @MainActor () -> Void
  /// The existing label.
  @ViewBuilder let label: () -> Label
  /// The window's focus restoration.
  @Environment(\.settingsAlertFocus) private var alertFocus
  /// Whether the button currently accepts input and focus.
  @Environment(\.isEnabled) private var isEnabled
  /// Distinguishes repeated Recover buttons and project rows.
  @State private var identity = UUID()
  /// The actual button's focus, rather than its inert row wrapper.
  @FocusState private var focused: Bool

  /// Draws the original button and remembers it before opening an alert.
  var body: some View {
    Button {
      if opensAlert, let alertFocus {
        alertFocus.remember(identity) { [binding = $focused] in binding.wrappedValue = true }
        focused = false
      }
      action()
    } label: {
      label()
    }
    .focused($focused)
    .onChange(of: isEnabled) { _, enabled in alertFocus?.setEnabled(enabled, for: identity) }
    .onDisappear { alertFocus?.forget(identity) }
  }
}

extension SettingsAlertButton where Label == Text {
  /// Creates a focus-restoring button with a plain title.
  ///
  /// - Parameters:
  ///   - title: The visible title.
  ///   - action: The existing action.
  init(_ title: String, action: @escaping @MainActor () -> Void) {
    self.action = action
    self.label = { Text(title) }
  }
}
