import SwiftUI

/// The menu bar panel: the helper's status, its desktop, facts and actions. It draws its state and sends what the
/// person does; it decides nothing.
package struct PanelView: View {
  /// What the panel shows.
  private let state: PanelState
  /// Receives what the person does.
  private let send: @MainActor (PanelEvent) -> Void

  /// Creates the panel.
  ///
  /// - Parameters:
  ///   - state: What the panel shows.
  ///   - send: Receives what the person does.
  package init(state: PanelState, send: @escaping @MainActor (PanelEvent) -> Void) {
    self.state = state
    self.send = send
  }

  /// The header, the stage, the cards and buttons, any log-out step and feedback, then the menu rows.
  package var body: some View {
    VStack(spacing: Spacing.l) {
      PanelHeader(status: state.status, reason: state.reason)
      stage
      if let recovery = state.recovery {
        RecoveryCard(recovery: recovery, isEnabled: state.feedback?.isWorking != true) { send(.recover) }
      }
      PanelFacts(facts: state.facts)
      PanelButtons(viewerLook: state.viewerLook, secondary: state.secondary, send: send)
      confirmation
      if let feedback = state.feedback {
        PanelFeedbackLine(feedback: feedback)
      }
      logOutAnyway
      SeparatorLine()
      PanelMenuList(menu: state.menu, send: send)
    }
    .padding(Spacing.xl)
    .frame(width: PanelLayout.width)
    .windowSurface()
  }

  /// The desktop, why there is none, or, logged out, the way to log in.
  @ViewBuilder private var stage: some View {
    switch state.stage {
    case .picture(let picture):
      PanelPicture(content: .picture(picture))
    case .placeholder(let text):
      PanelPicture(content: .placeholder(text))
    case .loggedOut(let text, let isEnabled):
      LogInCard(detail: text) { send(.logIn) }
        .disabled(!isEnabled)
    }
  }

  /// The log-out confirmation, while the person is asked.
  @ViewBuilder private var confirmation: some View {
    switch state.logOut {
    case .confirming(let isEnabled):
      LogOutConfirmation(cancel: { send(.cancelLogOut) }, logOut: { send(.logOut) })
        .disabled(!isEnabled)
    case .none, .held:
      EmptyView()
    }
  }

  /// Log Out Anyway, after an app held the last log-out up.
  @ViewBuilder private var logOutAnyway: some View {
    switch state.logOut {
    case .held(let isEnabled):
      Button("Log Out Anyway") { send(.forceLogOut) }
        .buttonStyle(.capsule(.glass, fill: true))
        .disabled(!isEnabled)
    case .none, .confirming:
      EmptyView()
    }
  }
}

#if DEBUG
  #Preview("Watching") {
    PanelView(state: PreviewStates.panelWatching) { _ in }
  }

  #Preview("Agent working") {
    PanelView(state: PreviewStates.panelAgentWorking) { _ in }
  }

  #Preview("In control") {
    PanelView(state: PreviewStates.panelInControl) { _ in }
  }

  #Preview("Recovering") {
    PanelView(state: PreviewStates.panelRecovering) { _ in }
  }

  #Preview("Input stopped") {
    PanelView(state: PreviewStates.panelInputStopped) { _ in }
  }

  #Preview("Logged out") {
    PanelView(state: PreviewStates.panelLoggedOut) { _ in }
  }

  #Preview("No picture") {
    PanelView(state: PreviewStates.panelNoPicture) { _ in }
  }

  #Preview("Confirm log-out") {
    PanelView(state: PreviewStates.panelConfirmLogOut) { _ in }
  }

  #Preview("Log-out held") {
    PanelView(state: PreviewStates.panelLogOutHeld) { _ in }
  }

  #Preview("Working message") {
    PanelView(state: PreviewStates.panelWorkingMessage) { _ in }
  }

  #Preview("Unavailable") {
    PanelView(state: PreviewStates.panelUnavailable) { _ in }
  }

  #Preview("Interactive") {
    @Previewable @State var state = PreviewStates.panelWatching
    InteractivePreview {
      Menu(String("State")) {
        ForEach(PreviewStates.panel) { preview in
          Button(preview.name) { state = preview.value }
        }
      }
      .fixedSize()
    } content: {
      PanelView(state: state) { event in
        state = PreviewStates.panel(state, applying: event)
      }
    }
  }
#endif
