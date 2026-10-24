import SwiftUI

/// The Settings window: the icon tabs at the top, the selected pane under them, and an alert over the whole window
/// when one is asked. It draws its state and sends what the person does; the General pane's values change through
/// `general`.
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
  ///   - send: Receives what the person does.
  public init(
    state: SettingsState,
    edit: @escaping @MainActor (SettingsGeneralState) -> Void,
    send: @escaping @MainActor (SettingsEvent) -> Void
  ) {
    self.state = state
    self.edit = edit
    self.send = send
  }

  /// The tabs over the selected pane, and the alert over both when one is asked.
  public var body: some View {
    ZStack {
      VStack(spacing: .zero) {
        IconTabs(state: state.tabs) { send(.selectTab($0)) }
          .frame(maxWidth: .infinity)
          .frame(height: SettingsLayout.tabRowHeight)
        pane
          .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
      }
      // Under an alert, nothing is reached by Tab or VoiceOver until it is answered.
      .disabled(state.alert != nil)
      .accessibilityHidden(state.alert != nil)
      AlertLayer(state: state.alert) { send($0 == .confirm ? .alertConfirm : .alertCancel) }
    }
    .windowSurface()
    // Each only when its words change, so a poll that reads the same says nothing.
    .onChange(of: state.alert?.title) { _, title in if let title { announce(title) } }
    .onChange(of: state.helper.status.label) { _, label in announce(label) }
    .onChange(of: state.helper.notice) { _, notice in if let notice { announce(notice.text) } }
    .onChange(of: state.agents.notice) { _, notice in if let notice { announce(notice.text) } }
    .onChange(of: state.about.answer) { _, answer in if let answer { announce(answer) } }
  }

  /// Tells VoiceOver what changed.
  ///
  /// - Parameter text: The words, final from the state.
  private func announce(_ text: String) {
    AccessibilityNotification.Announcement(text)
      .post()
  }

  /// The selected pane: a form scrolls when its notes make it taller than the window; About fills it, centred.
  @ViewBuilder private var pane: some View {
    switch state.tabs.selected {
    case .general: SettingsForm { GeneralPane(state: Binding(get: { state.general }, set: edit)) }
    case .helper: SettingsForm { HelperPane(state: state.helper, send: send) }
    case .agents: SettingsForm { AgentsPane(state: state.agents, send: send) }
    case .about:
      AboutPane(state: state.about, send: send)
        .padding(SettingsLayout.paneInsets)
    }
  }
}

#if DEBUG
  #Preview("General") {
    SettingsView(state: PreviewStates.settingsGeneral, edit: { _ in }, send: { _ in })
      .frame(width: SettingsLayout.window.width, height: SettingsLayout.window.height)
  }

  #Preview("Helper") {
    SettingsView(state: PreviewStates.settingsHelper, edit: { _ in }, send: { _ in })
      .frame(width: SettingsLayout.window.width, height: SettingsLayout.window.height)
  }

  #Preview("Forget") {
    SettingsView(state: PreviewStates.settingsForget, edit: { _ in }, send: { _ in })
      .frame(width: SettingsLayout.window.width, height: SettingsLayout.window.height)
  }

  #Preview("Interactive") {
    @Previewable @State var state = PreviewStates.settingsGeneral
    SettingsView(state: state, edit: { state.general = $0 }) { event in
      state = PreviewStates.settings(state, applying: event)
    }
    .frame(width: SettingsLayout.window.width, height: SettingsLayout.window.height)
  }
#endif
