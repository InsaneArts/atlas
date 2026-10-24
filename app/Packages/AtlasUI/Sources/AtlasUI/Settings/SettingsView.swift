import SwiftUI

/// The Settings window: the pane's name and the tabs at the top, the selected pane under them, and an alert over the
/// whole window when one is asked. It draws its state and sends what the person does, and how tall each pane draws, so
/// the window can fit it; the General pane's values change through `edit`.
public struct SettingsView: View {
  /// What the window shows.
  private let state: SettingsState
  /// Receives the General pane as the person changed it; the controller writes each value where it belongs.
  private let edit: @MainActor (SettingsGeneralState) -> Void
  /// Receives what the person does.
  private let send: @MainActor (SettingsEvent) -> Void

  /// Creates the window's view.
  ///
  /// - Parameters:
  ///   - state: What the window shows.
  ///   - edit: Receives the General pane as the person changed it.
  ///   - send: Receives what the person does, and each pane's height.
  public init(
    state: SettingsState,
    edit: @escaping @MainActor (SettingsGeneralState) -> Void,
    send: @escaping @MainActor (SettingsEvent) -> Void
  ) {
    self.state = state
    self.edit = edit
    self.send = send
  }

  /// The header over the selected pane, and the alert over both when one is asked.
  public var body: some View {
    ZStack {
      VStack(spacing: .zero) {
        SettingsHeader(title: state.title, tabs: state.tabs) { send(.selectTab($0)) }
        SettingsPane(state: state, edit: edit, send: send)
      }
      // Under an alert, nothing is reached by Tab or VoiceOver until it is answered.
      .disabled(state.alert != nil)
      .accessibilityHidden(state.alert != nil)
      AlertLayer(state: state.alert) { send($0 == .confirm ? .alertConfirm : .alertCancel) }
    }
    .windowSurface()
    // The header sits in the title bar, beside the window buttons, as v3 draws it.
    .ignoresSafeArea(.container, edges: .top)
    // Each only when its words change, so a poll that reads the same says nothing.
    .onChange(of: state.alert?.shownTitle) { _, title in if let title { announce(title) } }
    .onChange(of: state.alert?.refusal) { _, refusal in if let refusal { announce(refusal) } }
    .onChange(of: state.helper.account.status.label) { _, label in announce(label) }
    .onChange(of: state.helper.waiting) { _, waiting in if let waiting { announce(waiting) } }
    .onChange(of: state.helper.notice) { _, notice in if let notice { announce(notice.text) } }
    .onChange(of: state.agents.notice) { _, notice in if let notice { announce(notice.text) } }
    .onChange(of: state.about.status) { _, status in if let status { announce(status.text) } }
  }

  /// Tells VoiceOver what changed.
  ///
  /// - Parameter text: The words, final from the state.
  private func announce(_ text: String) {
    AccessibilityNotification.Announcement(text)
      .post()
  }
}

#if DEBUG
  #Preview("General") {
    SettingsView(state: PreviewStates.settingsGeneral, edit: { _ in }, send: { _ in })
      .frame(width: SettingsLayout.width, height: SettingsLayout.firstHeight)
  }

  #Preview("Helper") {
    SettingsView(state: PreviewStates.settingsHelper, edit: { _ in }, send: { _ in })
      .frame(width: SettingsLayout.width, height: SettingsLayout.firstHeight)
  }

  #Preview("Forget") {
    SettingsView(state: PreviewStates.settingsForget, edit: { _ in }, send: { _ in })
      .frame(width: SettingsLayout.width, height: SettingsLayout.firstHeight)
  }

  #Preview("Interactive") {
    @Previewable @State var state = PreviewStates.settingsGeneral
    SettingsView(state: state, edit: { state.general = $0 }) { event in
      state = PreviewStates.settings(state, applying: event)
    }
    .frame(width: SettingsLayout.width, height: SettingsLayout.firstHeight)
  }
#endif
