import SwiftUI

/// The Settings sidebar, stationary navigation header, existing pane content and window-wide alert.
public struct SettingsView: View {
  /// What the window shows.
  private let state: SettingsState
  /// Receives the General pane as the person changed it; the controller writes each value where it belongs.
  private let edit: @MainActor (SettingsGeneralState) -> Void
  /// Receives what the person does.
  private let send: @MainActor (SettingsEvent) -> Void

  /// Receives explicit results and alerts for VoiceOver, independently of routine helper status.
  private let announce: @MainActor (String) -> Void
  /// The single outgoing surface; another navigation replaces it immediately.
  @State private var outgoing: SettingsSurface?

  /// Creates the window's view.
  ///
  /// - Parameters:
  ///   - state: What the window shows.
  ///   - edit: Receives the General pane as the person changed it.
  ///   - announce: Posts explicit results and alerts to VoiceOver.
  ///   - send: Receives what the person does.
  public init(
    state: SettingsState,
    edit: @escaping @MainActor (SettingsGeneralState) -> Void,
    announce: @escaping @MainActor (String) -> Void = {
      AccessibilityNotification.Announcement($0)
        .post()
    },
    send: @escaping @MainActor (SettingsEvent) -> Void
  ) {
    self.state = state
    self.edit = edit
    self.send = send
    self.announce = announce
  }

  /// The stationary shell and selected pane beneath the window-wide alert.
  public var body: some View {
    ZStack {
      HStack(spacing: .zero) {
        SettingsSidebar(state: state, send: send)
        VStack(spacing: .zero) {
          SettingsHeader(
            title: state.title,
            canGoBack: state.canGoBack && !state.search.isActive,
            canGoForward: state.canGoForward && !state.search.isActive,
            send: send
          )
          ZStack(alignment: .top) {
            SettingsPane(state: state, edit: edit, send: send, outgoing: outgoing?.destination)
            SettingsSearchPresentation(
              state: state.search,
              send: send,
              retained: surface == .search || outgoing == .search
            )
          }
        }
      }
      // Under an alert, nothing is reached by Tab or VoiceOver until it is answered.
      .disabled(state.alert != nil)
      .accessibilityHidden(state.alert != nil)
      AlertLayer(state: state.alert) { send($0 == .confirm ? .alertConfirm : .alertCancel) }
    }
    .windowSurface()
    // The sidebar reserves traffic-light space in the full-size content view.
    .ignoresSafeArea(.container, edges: .top)
    .onChange(of: surface) { previous, _ in outgoing = previous }
    // Each only when its words change, so a poll that reads the same says nothing.
    .onChange(of: state.alert?.shownTitle) { _, title in if let title { announce(title) } }
    .onChange(of: state.alert?.refusal) { _, refusal in if let refusal { announce(refusal) } }
    .onChange(of: state.helper.waiting) { _, waiting in if let waiting { announce(waiting) } }
    .onChange(of: state.helper.buttons.isChecking) { _, checking in
      if !checking {
        let ready = !state.helper.ready.isEmpty && state.helper.ready.allSatisfy { $0.item.state == .ready }
        announce(state.helper.readyNote?.text ?? (ready ? "Checked. All ready." : "Checked. Not everything is ready."))
      }
    }
    .onChange(of: state.helper.notice) { _, notice in if let notice { announce(notice.text) } }
    .onChange(of: state.agents.notice) { _, notice in if let notice { announce(notice.text) } }
    .onChange(of: state.about.status) { _, status in if let status { announce(status.text) } }
    .onChange(of: state.search.query) { _, _ in
      if state.search.isActive {
        announce(state.search.results.isEmpty ? "No settings found" : "Search results available")
      }
    }
  }

  /// The latest requested destination, without any animation queue.
  private var surface: SettingsSurface {
    state.search.isActive ? .search : .page(state.location)
  }

}

#if DEBUG
  #Preview("Search states") {
    ScrollView {
      VStack(spacing: Spacing.xxl) {
        ForEach(PreviewStates.settingsSearch) { preview in
          SettingsView(state: preview.value, edit: { _ in }, send: { _ in })
            .frame(width: SettingsLayout.width, height: SettingsLayout.firstHeight)
            .previewDisplayName(preview.name)
        }
      }
    }
  }

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

  #Preview("Minimum viewport") {
    SettingsView(state: PreviewStates.settingsHelper, edit: { _ in }, send: { _ in })
      .frame(width: SettingsLayout.minimumWidth, height: SettingsLayout.minimumHeight)
  }

  #Preview("Long account and connecting subtitle") {
    SettingsView(state: PreviewStates.settingsLongHelper, edit: { _ in }, send: { _ in })
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
