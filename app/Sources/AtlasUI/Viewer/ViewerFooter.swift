import SwiftUI

/// What to do next, as the onboarding's footer: where things stand on the left, the next step in white on the right.
struct ViewerFooter: View {
  /// What the footer shows.
  let state: ViewerFooterState
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  /// The hint, then Recover… when set aside, Stop or Resume Agent, pause, Keys and the control capsule.
  var body: some View {
    VStack(spacing: .zero) {
      SeparatorLine()
      HStack(spacing: Spacing.ml) {
        Text(state.hint)
          .font(Typography.font(.callout))
          .foregroundStyle(Color.textTertiary)
          .lineLimit(1)
          .truncationMode(.tail)
          .accessibilityIdentifier("hint")
        Spacer(minLength: Spacing.l)
        if state.offersRecovery {
          Button("Recover…") { send(.showRecovery) }
            .buttonStyle(.capsule(.glass))
        }
        switch state.agent {
        case .stop:
          Button("Stop Agent") { send(.stopAgent) }
            .buttonStyle(.capsule(.glass))
            .help("End what the agent is doing in the helper, and let it do nothing more until you resume")
        case .resume(let isEnabled):
          Button("Resume Agent") { send(.resumeAgent) }
            .buttonStyle(.capsule(.glass))
            .disabled(!isEnabled)
            .help("Let agents act in the helper again")
        case nil:
          EmptyView()
        }
        Button {
          send(.togglePause)
        } label: {
          pauseSymbol
        }
        .buttonStyle(.circle())
        .keyboardShortcut("p", modifiers: [.command, .shift])
        .help(state.pause.help)
        .accessibilityLabel(state.pause.label)
        // Keys the Mac keeps for itself (⌘⇥, ⌘Space…), offered in a menu the shell pops up, which leaves the
        // window in front and control where it is.
        Button {
          send(.showKeys)
        } label: {
          Label("Keys", systemImage: "keyboard")
        }
        .buttonStyle(.capsule(.glass))
        .disabled(!state.keysEnabled)
        .help("Send a key the Mac would keep for itself, such as ⌘⇥")
        // While in control the desktop's key monitor answers ⌃⇧⎋ first and swallows it; this shortcut takes over
        // only when the desktop does not have the keyboard, or while watching. The monitor swallows a held chord's
        // repeats, so this shortcut never sees them.
        Button {
          send(state.control.event)
        } label: {
          Text(state.control.title)
        }
        .buttonStyle(.capsule())
        .keyboardShortcut(.escape, modifiers: [.control, .shift])
        .disabled(!state.control.isEnabled)
        .help(state.control.help)
      }
      .padding(.horizontal, ViewerLayout.barInset)
      .frame(height: ViewerLayout.footerHeight)
    }
  }

  /// Play while paused, pause otherwise.
  private var pauseSymbol: some View {
    Image(systemName: state.pause.isOn ? "play.fill" : "pause.fill")
  }
}

#if DEBUG
  #Preview("Control") {
    ViewerFooter(state: PreviewStates.viewerControl.footer) { _ in }
      .frame(width: PreviewStates.viewerSize.width)
      .sampleSurface()
  }
#endif
