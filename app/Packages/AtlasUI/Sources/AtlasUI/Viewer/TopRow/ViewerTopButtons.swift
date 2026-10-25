import SwiftUI

/// The top row's buttons, in order: pause, Keys, Stop, Resume, the white capsule, ⋯ and Details, 30 points high and 8
/// apart. Only the actions that apply are drawn: each stands in its slot, growing in from its leading edge as it
/// applies and shrinking away as it stops applying, its neighbours closing the gap (``ActionSlot``). Each tooltip
/// names its menu item and shortcut, so every shortcut can be found from its button.
struct ViewerTopButtons: View {
  /// The buttons' states.
  let actions: ViewerTopActions
  /// Whether Keys, Stop and Resume keep their words; a narrow window draws their symbols alone.
  let actionWords: Bool
  /// Whether the picture's pause stands in the row; a narrow window leaves it to the View menu.
  let showsPause: Bool
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  /// Where Keys is, in the window, so its menu opens under it.
  @State private var keysFrame = CGRect.zero
  /// Where ⋯ is, in the window, so its menu opens under it.
  @State private var moreFrame = CGRect.zero

  /// The buttons that apply, each in its slot.
  var body: some View {
    HStack(spacing: Spacing.s) {
      ViewerPauseSlot(pause: pause, send: send)
      keys
        .actionSlot(actions.keys != .hidden)
      wordOrSymbol(String("Stop"), symbol: .stop, event: .stopAgent, isEnabled: true)
        .accessibilityLabel(ViewerAgentButton.stop.accessibilityLabel)
        .help(actions.agentHelp)
        .actionSlot(actions.agent == .stop)
      wordOrSymbol(String("Resume"), symbol: .resume, event: .resumeAgent, isEnabled: resumeEnabled)
        .accessibilityLabel(ViewerAgentButton.resume(isEnabled: resumeEnabled).accessibilityLabel)
        .help(actions.agentHelp)
        .actionSlot(isResume)
      ViewerMainButton(control: actions.control, refusals: actions.refusals, send: send)
      more
        .actionSlot(actions.more != nil)
      details
    }
    .fixedSize()
    .animatesActionSlots(pause != nil)
    .animatesActionSlots(actions.keys != .hidden)
    .animatesActionSlots(actions.agent == .stop)
    .animatesActionSlots(isResume)
    .animatesActionSlots(actions.control != nil)
    .animatesActionSlots(actions.more != nil)
  }

  /// The picture's pause, where this layout keeps it.
  private var pause: ViewerToggle? {
    showsPause ? actions.pause : nil
  }

  /// Keys, while this window holds control: the keys this Mac keeps for itself, in a menu under it; its symbol and
  /// word on a glass capsule, or its symbol in a glass circle in a narrow window.
  private var keys: some View {
    Group {
      if actionWords {
        Button {
          send(.showKeys(below: keysFrame))
        } label: {
          HStack(spacing: Spacing.xsPlus) {
            Image(systemName: SymbolName.keyboard.rawValue)
              .font(Typography.symbol(.control, weight: .medium))
            Text(verbatim: "Keys")
          }
        }
        .buttonStyle(.capsule(.glass, size: .small, isOn: keysOpen))
      } else {
        Button {
          send(.showKeys(below: keysFrame))
        } label: {
          Image(systemName: SymbolName.keyboard.rawValue)
        }
        .buttonStyle(.capsule(.glass, size: .small, outline: .circle, isOn: keysOpen))
      }
    }
    .onGeometryChange(for: CGRect.self) {
      $0.frame(in: .global)
    } action: {
      keysFrame = $0
    }
    .help(String("Send a key your Mac would keep, such as ⌘⇥"))
    .accessibilityLabel(Text(verbatim: "Keys"))
    .accessibilityHint(Text(verbatim: "Opens a menu"))
  }

  /// Stop or Resume: its word on a glass capsule, or its symbol in a glass circle in a narrow window.
  ///
  /// - Parameters:
  ///   - title: Its word.
  ///   - symbol: Its symbol.
  ///   - event: What pressing it sends.
  ///   - isEnabled: Whether it can be pressed.
  /// - Returns: The button.
  @ViewBuilder private func wordOrSymbol(
    _ title: String,
    symbol: SymbolName,
    event: ViewerEvent,
    isEnabled: Bool
  ) -> some View {
    if actionWords {
      Button(title) { send(event) }
        .buttonStyle(.capsule(.glass, size: .small))
        .disabled(!isEnabled)
    } else {
      Button {
        send(event)
      } label: {
        Image(systemName: symbol.rawValue)
      }
      .buttonStyle(.capsule(.glass, size: .small, outline: .circle))
      .disabled(!isEnabled)
    }
  }

  /// Whether Keys's menu is open, which draws it pressed.
  private var keysOpen: Bool {
    actions.keys == .shown(isOpen: true)
  }

  /// ⋯: the commands that act on this window's picture, in a menu under it.
  private var more: some View {
    Button {
      send(.showMore(below: moreFrame))
    } label: {
      Image(systemName: SymbolName.more.rawValue)
    }
    .buttonStyle(.capsule(.glass, size: .small, outline: .circle, isOn: actions.more?.isOn == true))
    .onGeometryChange(for: CGRect.self) {
      $0.frame(in: .global)
    } action: {
      moreFrame = $0
    }
    .help(actions.more?.help ?? "")
    .accessibilityLabel(actions.more?.label ?? "")
    .accessibilityHint(Text(verbatim: "Opens a menu"))
  }

  /// Details: the inspector beside the screen.
  private var details: some View {
    Button {
      send(.toggleDetails)
    } label: {
      Image(systemName: SymbolName.sidebar.rawValue)
    }
    .buttonStyle(.capsule(.glass, size: .small, outline: .circle, isOn: actions.details.isOn))
    .help(actions.details.help)
    .accessibilityLabel(actions.details.label)
    .accessibilityAddTraits(actions.details.isOn ? .isSelected : [])
  }

  /// Whether Resume stands in place of Stop.
  private var isResume: Bool {
    guard case .resume = actions.agent else { return false }
    return true
  }

  /// Whether Resume can be pressed: it waits while recovery runs.
  private var resumeEnabled: Bool {
    guard case .resume(let isEnabled) = actions.agent else { return false }
    return isEnabled
  }
}

#if DEBUG
  #Preview("Stopped") {
    ViewerTopButtons(actions: PreviewStates.viewerStopped.top.actions, actionWords: true, showsPause: true) { _ in }
      .sampleSurface()
  }

  #Preview("Interactive") {
    @Previewable @State var state = PreviewStates.viewerWorking
    @Previewable @State var actionWords = true
    @Previewable @State var showsPause = true
    InteractivePreview {
      Menu(String("State")) {
        ForEach(PreviewStates.viewer) { preview in
          Button(preview.name) { state = preview.value }
        }
      }
      .fixedSize()
      Toggle(String("Words"), isOn: $actionWords)
      Toggle(String("Pause"), isOn: $showsPause)
    } content: {
      ViewerTopButtons(actions: state.top.actions, actionWords: actionWords, showsPause: showsPause) { event in
        state = PreviewStates.viewer(state, applying: event)
      }
    }
  }
#endif
