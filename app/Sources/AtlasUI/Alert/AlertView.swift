import SwiftUI

/// The alert window's content: one alert in the widget's look on a clear ground, with room around it for its shadow.
/// Atlas shows it when the menu bar panel asks to recover, switch to or log out the helper while no main window is
/// open; the main window shows the same alert over its own scrim instead.
package struct AlertView: View {
  /// What the alert says and offers.
  private let state: OrbitAlertState<AlertChoice>
  /// Receives the button the person presses.
  private let send: @MainActor (AlertChoice) -> Void

  /// Creates the alert window's content.
  ///
  /// - Parameters:
  ///   - state: What the alert says and offers.
  ///   - send: Receives the button the person presses.
  package init(state: OrbitAlertState<AlertChoice>, send: @escaping @MainActor (AlertChoice) -> Void) {
    self.state = state
    self.send = send
  }

  package var body: some View {
    OrbitAlert(state: state, send: send)
      .padding(AlertLayout.shadowRoom)
      .foregroundStyle(Color.textPrimary)
      .environment(\.colorScheme, .dark)
  }
}

#if DEBUG
  #Preview("Recover") {
    AlertView(state: PreviewStates.alertRecover) { _ in }
      .sampleSurface()
  }

  #Preview("Log out") {
    AlertView(state: PreviewStates.alertLogOut) { _ in }
      .sampleSurface()
  }

  #Preview("Interactive") {
    @Previewable @State var state = PreviewStates.alertRecover
    InteractivePreview {
      Menu(String("Alert")) {
        ForEach(PreviewStates.alert) { preview in
          Button(preview.name) { state = preview.value }
        }
      }
      .fixedSize()
      Toggle(String("Busy"), isOn: $state.isBusy)
    } content: {
      AlertView(state: state) { _ in
        state.isBusy = true
      }
    }
  }
#endif
