import SwiftUI

/// The top row's buttons, in order: Clear, pause, Keys, Stop or Resume, the white capsule, ⋯ and Details. Each says
/// what it does in its tooltip; the shortcuts live in the main menu alone.
struct ViewerTopButtons: View {
  /// The buttons' states.
  let actions: ViewerTopActions
  /// How far the row draws in for the window's width.
  let fit: ViewerTopRowFit
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  /// Where Keys is, in the window, so its menu opens under it.
  @State private var keysFrame = CGRect.zero
  /// Where ⋯ is, in the window, so its menu opens under it.
  @State private var moreFrame = CGRect.zero

  var body: some View {
    HStack(spacing: Spacing.s) {
      if actions.showsClear {
        Button("Clear") { send(.clearOutline) }
          .buttonStyle(.capsule(.glass))
          .help("Stop showing the window")
      }
      Button {
        send(.togglePause)
      } label: {
        PauseGlyph(isPaused: actions.pause.isOn)
      }
      .buttonStyle(.circle(isOn: actions.pause.isOn))
      .help(actions.pause.help)
      .accessibilityLabel(actions.pause.label)
      keys
      agent
      Button(fit < .shortWords ? actions.control.title : actions.control.shortTitle) { send(actions.control.event) }
        .buttonStyle(.capsule())
        .disabled(!actions.control.isEnabled)
        .help(actions.control.help)
        .accessibilityLabel(actions.control.title)
      Button {
        send(.showMore(below: moreFrame))
      } label: {
        Image(systemName: "ellipsis")
      }
      .buttonStyle(.circle(isOn: actions.more.isOn))
      .onGeometryChange(for: CGRect.self) {
        $0.frame(in: .global)
      } action: {
        moreFrame = $0
      }
      .help(actions.more.help)
      .accessibilityLabel(actions.more.label)
      Button {
        send(.toggleDetails)
      } label: {
        Image(systemName: "info.circle")
      }
      .buttonStyle(.circle(isOn: actions.details.isOn))
      .help(actions.details.help)
      .accessibilityLabel(actions.details.label)
      .accessibilityAddTraits(actions.details.isOn ? .isSelected : [])
    }
    .fixedSize()
  }

  /// Keys, pressed while in control and faded otherwise; its word goes in a narrow window.
  @ViewBuilder private var keys: some View {
    if actions.keys != .hidden {
      Button {
        send(.showKeys(below: keysFrame))
      } label: {
        HStack(spacing: Spacing.xsPlus) {
          Image(systemName: "keyboard")
          if fit < .keysGlyph {
            Text("Keys")
          }
        }
      }
      .buttonStyle(.capsule(actions.keys == .pressed ? .pressed : .glass))
      .disabled(actions.keys == .disabled)
      .onGeometryChange(for: CGRect.self) {
        $0.frame(in: .global)
      } action: {
        keysFrame = $0
      }
      .help("Send a key your Mac would keep, such as ⌘⇥")
      .accessibilityLabel("Keys")
    }
  }

  /// Stop while an agent works, or Resume once the person stopped it.
  @ViewBuilder private var agent: some View {
    switch actions.agent {
    case .stop:
      Button("Stop") { send(.stopAgent) }
        .buttonStyle(.capsule(.glass))
        .help(actions.agentHelp)
    case .resume(let isEnabled):
      Button("Resume") { send(.resumeAgent) }
        .buttonStyle(.capsule(.glass))
        .disabled(!isEnabled)
        .help(actions.agentHelp)
    case nil:
      EmptyView()
    }
  }
}

#if DEBUG
  #Preview("Stopped") {
    ViewerTopButtons(actions: PreviewStates.viewerStopped.top.actions, fit: .regular) { _ in }
      .sampleSurface()
  }
#endif
