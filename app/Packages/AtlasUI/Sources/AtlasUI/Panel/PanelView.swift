import SwiftUI

/// The menu bar panel: the helper's status as its heading, its screen, the newest actions or, while two or more are
/// live, the host sessions, and the actions on the agent. It draws its state and sends what the person does; it decides
/// nothing. A state change moves every piece at once on ``PanelLayout/change``: the word, the line, the picture's look
/// and the buttons ease together, so nothing ripples down the panel on every flip. It fills the popover that holds it,
/// whose corners, glass and shadow are the system's; `panelCard()` draws its own where no popover does.
public struct PanelView: View {
  /// What the panel shows.
  private let state: PanelState
  /// Receives what the person does.
  private let send: @MainActor (PanelEvent) -> Void
  @Environment(\.accessibilityReduceTransparency) private var reduceTransparency

  /// Creates the panel.
  ///
  /// - Parameters:
  ///   - state: What the panel shows.
  ///   - send: Receives what the person does.
  public init(state: PanelState, send: @escaping @MainActor (PanelEvent) -> Void) {
    self.state = state
    self.send = send
  }

  /// The header, the picture, Sessions while there are any, else Recent when there is any, then the buttons.
  public var body: some View {
    VStack(alignment: .leading, spacing: PanelLayout.gap) {
      PanelHeader(status: state.status, notice: state.notice, isMoreOpen: state.isMoreOpen, send: send)
      PanelPicture(state: state.picture, send: send)
        .animation(PanelLayout.change, value: state.picture.look)
      if !state.sessions.isEmpty {
        PanelSessions(entries: state.sessions, more: state.moreSessions, send: send)
      } else if !state.recent.isEmpty {
        PanelRecent(entries: state.recent, send: send)
      }
      PanelControlRow(controls: state.controls, send: send)
        .animation(PanelLayout.change, value: state.controls.main)
    }
    .padding(PanelLayout.inset)
    .frame(width: PanelLayout.width)
    .background(
      PanelLayout.ground
        .opacity(PanelLayout.groundOpacity(reduceTransparency: reduceTransparency))
    )
    .foregroundStyle(Color.textPrimary)
    .environment(\.colorScheme, .dark)
    // Only a change is spoken: a poll that reads the same announcement or notice again says nothing.
    .onChange(of: state.announcement) { _, text in if let text { announce(text) } }
    .onChange(of: state.notice) { _, notice in if let notice { announce(notice) } }
  }

  /// Has VoiceOver say `text`, as a change the person did not move to.
  ///
  /// - Parameter text: What to say.
  private func announce(_ text: String) {
    AccessibilityNotification.Announcement(text)
      .post()
  }
}

#if DEBUG
  #Preview("Idle") {
    PanelView(state: PreviewStates.panelIdle) { _ in }
      .panelCard()
      .padding(Spacing.xxxl)
  }

  #Preview("Working") {
    PanelView(state: PreviewStates.panelWorking) { _ in }
      .panelCard()
      .padding(Spacing.xxxl)
  }

  #Preview("You have control") {
    PanelView(state: PreviewStates.panelControl) { _ in }
      .panelCard()
      .padding(Spacing.xxxl)
  }

  #Preview("Needs you") {
    PanelView(state: PreviewStates.panelNeedsYou) { _ in }
      .panelCard()
      .padding(Spacing.xxxl)
  }

  #Preview("Stopped") {
    PanelView(state: PreviewStates.panelStopped) { _ in }
      .panelCard()
      .padding(Spacing.xxxl)
  }

  #Preview("Logged out") {
    PanelView(state: PreviewStates.panelLoggedOut) { _ in }
      .panelCard()
      .padding(Spacing.xxxl)
  }

  #Preview("Not ready") {
    PanelView(state: PreviewStates.panelNotReady) { _ in }
      .panelCard()
      .padding(Spacing.xxxl)
  }

  #Preview("Connecting") {
    PanelView(state: PreviewStates.panelConnecting) { _ in }
      .panelCard()
      .padding(Spacing.xxxl)
  }

  #Preview("Picture paused") {
    PanelView(state: PreviewStates.panelPaused) { _ in }
      .panelCard()
      .padding(Spacing.xxxl)
  }

  #Preview("Sessions") {
    PanelView(state: PreviewStates.panelSessions) { _ in }
      .panelCard()
      .padding(Spacing.xxxl)
  }

  #Preview("Sessions waiting") {
    PanelView(state: PreviewStates.panelSessionsWaiting) { _ in }
      .panelCard()
      .padding(Spacing.xxxl)
  }

  #Preview("Sessions need you") {
    PanelView(state: PreviewStates.panelSessionsNeedsYou) { _ in }
      .panelCard()
      .padding(Spacing.xxxl)
  }

  #Preview("In use elsewhere") {
    PanelView(state: PreviewStates.panelInUseElsewhere) { _ in }
      .panelCard()
      .padding(Spacing.xxxl)
  }

  #Preview("Interactive") {
    @Previewable @State var state = PreviewStates.panelIdle
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
      .panelCard()
    }
  }
#endif
