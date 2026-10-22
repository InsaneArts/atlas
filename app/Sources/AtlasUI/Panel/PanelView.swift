import SwiftUI

/// The menu bar panel (MacPanel): the helper's status, its screen, the newest actions and one row of buttons. It
/// draws its state and sends what the person does; it decides nothing. It fills the popover that holds it, whose
/// corners and shadow are the system's; `panelCard()` draws the board's own where no popover does.
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

  /// The header, the line, the picture, Recent when there is any, then the buttons.
  package var body: some View {
    VStack(alignment: .leading, spacing: PanelLayout.gap) {
      PanelHeader(status: state.status)
      line
      PanelPicture(state: state.picture, send: send)
      if !state.recent.isEmpty {
        PanelRecent(entries: state.recent, send: send)
      }
      PanelControlRow(controls: state.controls, send: send)
    }
    .padding(PanelLayout.inset)
    .frame(width: PanelLayout.width)
    .background(PanelLayout.ground)
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

  /// The status's line, or what the last action said when it did not work, in one line; nothing when both are
  /// empty.
  @ViewBuilder private var line: some View {
    if let notice = state.notice {
      lineText(notice)
        .foregroundStyle(Color.textAttention)
    } else if !state.status.line.isEmpty {
      lineText(state.status.line)
        .foregroundStyle(Color.textSecondary)
    }
  }

  /// One line of the line row's words, cut short with an ellipsis.
  ///
  /// - Parameter text: The words.
  /// - Returns: The line.
  private func lineText(_ text: String) -> some View {
    Text(text)
      .font(Typography.font(.callout))
      .lineLimit(1)
      .truncationMode(.tail)
  }
}

#if DEBUG
  #Preview("Idle") {
    PanelView(state: PreviewStates.panelIdle) { _ in }
      .panelCard()
      .padding(Spacing.page)
  }

  #Preview("Working") {
    PanelView(state: PreviewStates.panelWorking) { _ in }
      .panelCard()
      .padding(Spacing.page)
  }

  #Preview("You have control") {
    PanelView(state: PreviewStates.panelControl) { _ in }
      .panelCard()
      .padding(Spacing.page)
  }

  #Preview("Needs you") {
    PanelView(state: PreviewStates.panelNeedsYou) { _ in }
      .panelCard()
      .padding(Spacing.page)
  }

  #Preview("Stopped") {
    PanelView(state: PreviewStates.panelStopped) { _ in }
      .panelCard()
      .padding(Spacing.page)
  }

  #Preview("Logged out") {
    PanelView(state: PreviewStates.panelLoggedOut) { _ in }
      .panelCard()
      .padding(Spacing.page)
  }

  #Preview("Not ready") {
    PanelView(state: PreviewStates.panelNotReady) { _ in }
      .panelCard()
      .padding(Spacing.page)
  }

  #Preview("Connecting") {
    PanelView(state: PreviewStates.panelConnecting) { _ in }
      .panelCard()
      .padding(Spacing.page)
  }

  #Preview("In use elsewhere") {
    PanelView(state: PreviewStates.panelInUseElsewhere) { _ in }
      .panelCard()
      .padding(Spacing.page)
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
