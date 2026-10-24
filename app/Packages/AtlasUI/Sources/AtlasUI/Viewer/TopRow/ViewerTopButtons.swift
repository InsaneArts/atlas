import SwiftUI

/// The top row's buttons (v3 D11), in order: Clear, pause, Keys, Stop, Resume, the white capsule, ⋯ and Details, 30
/// points high and 8 apart. Only the actions that apply are drawn: each stands in its slot, growing in from its
/// leading edge as it applies and shrinking away as it stops applying, its neighbours closing the gap (``ActionSlot``).
/// Each says what it does in its tooltip; the shortcuts live in the main menu alone.
struct ViewerTopButtons: View {
  /// The buttons' states.
  let actions: ViewerTopActions
  /// Whether Keys, Stop and Resume keep their words; a narrow window draws their symbols alone (v3 V15).
  let actionWords: Bool
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  /// Where Keys is, in the window, so its menu opens under it.
  @State private var keysFrame = CGRect.zero
  /// Where ⋯ is, in the window, so its menu opens under it.
  @State private var moreFrame = CGRect.zero

  /// The buttons that apply, each in its slot.
  var body: some View {
    HStack(spacing: Spacing.s) {
      wordOrSymbol(String("Clear"), symbol: .clear, event: .clearOutline, isEnabled: true)
        .help(String("Stop showing the window"))
        .actionSlot(actions.showsClear)
      PauseSlot(pause: actions.pause, send: send)
      keys
        .actionSlot(actions.keys != .hidden)
      wordOrSymbol(String("Stop"), symbol: .stop, event: .stopAgent, isEnabled: true)
        .help(actions.agentHelp)
        .actionSlot(actions.agent == .stop)
      wordOrSymbol(String("Resume"), symbol: .resume, event: .resumeAgent, isEnabled: resumeEnabled)
        .help(actions.agentHelp)
        .actionSlot(isResume)
      MainButton(control: actions.control, refusals: actions.refusals, send: send)
      more
      details
    }
    .fixedSize()
    .animatesActionSlots(actions.showsClear)
    .animatesActionSlots(actions.pause != nil)
    .animatesActionSlots(actions.keys != .hidden)
    .animatesActionSlots(actions.agent == .stop)
    .animatesActionSlots(isResume)
    .animatesActionSlots(actions.control != nil)
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
  }

  /// Clear, Stop or Resume: its word on a glass capsule, or its symbol in a glass circle in a narrow window.
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
      .accessibilityLabel(Text(title))
    }
  }

  /// Whether Keys's menu is open, which draws it pressed.
  private var keysOpen: Bool {
    actions.keys == .shown(isOpen: true)
  }

  /// ⋯: the picture's and the helper's commands, in a menu under it.
  private var more: some View {
    Button {
      send(.showMore(below: moreFrame))
    } label: {
      Image(systemName: SymbolName.more.rawValue)
    }
    .buttonStyle(.capsule(.glass, size: .small, outline: .circle, isOn: actions.more.isOn))
    .onGeometryChange(for: CGRect.self) {
      $0.frame(in: .global)
    } action: {
      moreFrame = $0
    }
    .help(actions.more.help)
    .accessibilityLabel(actions.more.label)
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

/// The picture's pause in its slot (v3 V11): pause turns into play by a symbol replace once the picture is paused, at
/// once under Reduce Motion. It pauses the picture only; work in the helper goes on.
private struct PauseSlot: View {
  /// The pause; nil while there is no picture to pause.
  let pause: ViewerToggle?
  /// Receives the press.
  let send: @MainActor (ViewerEvent) -> Void
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  var body: some View {
    Button {
      send(.togglePause)
    } label: {
      Image(systemName: (pause?.isOn == true ? SymbolName.play : SymbolName.pause).rawValue)
        .contentTransition(.symbolEffect(.replace))
        .animation(Motion.animation(Motion.state, reduceMotion: reduceMotion), value: pause?.isOn)
    }
    .buttonStyle(.capsule(.glass, size: .small, outline: .circle))
    .help(pause?.help ?? "")
    .accessibilityLabel(pause?.label ?? "")
    .actionSlot(pause != nil)
  }
}

/// The white capsule in its slot (v3 X5, V12): Take Control, Give Back or Recover…, its title morphing in place in a
/// capsule at least 116 wide, so the row does not move. A refused Take Control shakes it once.
private struct MainButton: View {
  /// The button; nil while the stage offers what to do instead.
  let control: ViewerControlButton?
  /// Take Control's refusals counted.
  let refusals: Int
  /// Receives the press.
  let send: @MainActor (ViewerEvent) -> Void
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  var body: some View {
    Button {
      if let control { send(control.event) }
    } label: {
      Text(control?.title ?? "")
        .contentTransition(.interpolate)
        .frame(minWidth: Layout.labelMinWidth)
        .animation(
          Motion.animation(Motion.arrive, reduceMotion: reduceMotion, reduced: Motion.reduced),
          value: control?.title
        )
    }
    .buttonStyle(.capsule(size: .small))
    .disabled(control?.isEnabled != true)
    .shakes(refusals)
    .help(control?.help ?? "")
    .accessibilityLabel(control?.title ?? "")
    .actionSlot(control != nil)
  }
}

/// The white capsule's own measure.
private enum Layout {
  /// 18: a capsule's inset each side of its title (``CapsuleButtonStyle``'s for a capsule that hugs its title).
  static let capsuleInset: CGFloat = 18
  /// 80: its title's least width, the capsule's 116 less its inset each side (v3 `min-width: 116px`).
  static let labelMinWidth = ViewerLayout.mainButtonMinWidth - capsuleInset * 2
}

#if DEBUG
  #Preview("Stopped") {
    ViewerTopButtons(actions: PreviewStates.viewerStopped.top.actions, actionWords: true) { _ in }
      .sampleSurface()
  }

  #Preview("Interactive") {
    @Previewable @State var state = PreviewStates.viewerWorking
    @Previewable @State var actionWords = true
    InteractivePreview {
      Menu(String("State")) {
        ForEach(PreviewStates.viewer) { preview in
          Button(preview.name) { state = preview.value }
        }
      }
      .fixedSize()
      Toggle(String("Words"), isOn: $actionWords)
    } content: {
      ViewerTopButtons(actions: state.top.actions, actionWords: actionWords) { event in
        state = PreviewStates.viewer(state, applying: event)
      }
    }
  }
#endif
