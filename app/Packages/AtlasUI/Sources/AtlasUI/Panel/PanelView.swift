import SwiftUI

/// The menu bar panel (MacPanel, v3 P1–P11): the helper's status as its heading, its screen, the newest actions and
/// the actions on the agent. It draws its state and sends what the person does; it decides nothing. A state change
/// cascades through it in one beat (v3 P7): the word and its dot, then the line, the picture and the buttons, each
/// animating on its own value 30 milliseconds after the one before, never the whole panel at once. It fills the popover
/// that holds it, whose corners, glass and shadow are the system's; `panelCard()` draws the board's own where no popover
/// does.
public struct PanelView: View {
  /// What the panel shows.
  private let state: PanelState
  /// Receives what the person does.
  private let send: @MainActor (PanelEvent) -> Void
  @Environment(\.accessibilityReduceMotion) private var reduceMotion
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

  /// The header, the picture, Recent when there is any, then the buttons.
  public var body: some View {
    VStack(alignment: .leading, spacing: PanelLayout.gap) {
      PanelHeader(status: state.status, notice: state.notice, isMoreOpen: state.isMoreOpen, send: send)
      PanelPicture(state: state.picture, send: send)
        .animation(PanelLayout.cascade(.picture, reduceMotion: reduceMotion), value: state.picture.look)
      if !state.recent.isEmpty {
        PanelRecent(entries: state.recent, send: send)
      }
      PanelControlRow(controls: state.controls, send: send)
        .animation(PanelLayout.cascade(.buttons, reduceMotion: reduceMotion), value: state.controls)
    }
    .padding(PanelLayout.inset)
    .frame(width: PanelLayout.width)
    .background(
      PanelLayout.ground
        .opacity(PanelLayout.groundOpacity(reduceTransparency: reduceTransparency))
    )
    .foregroundStyle(Color.textPrimary)
    .environment(\.colorScheme, .dark)
    // Only a change is spoken: a poll that reads the same status or notice again says nothing.
    .onChange(of: state.status.label) { _, label in announce(label) }
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
