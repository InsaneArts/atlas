import SwiftUI

/// Stop or Resume in the footer, one capsule in its slot at one width, the wider of the two labels', so pressing it
/// never moves it or the pause beside it: its symbol and word swap together with no overlap (``FooterLabel``). In a
/// narrow card it stands as a circle round its symbol (``FooterFold``). Its tooltip names its menu item and shortcut.
struct FooterAgentButton: View {
  /// Stop or Resume; nil shows neither.
  let agent: ViewerAgentButton?
  /// Its tooltip, with its shortcut.
  let help: String
  /// How the footer draws in a narrow card.
  var fold = FooterFold(.full)
  /// Receives the press.
  let send: @MainActor (ViewerEvent) -> Void

  /// Stop's label.
  static let stop = ViewerControlLabel("Stop", symbol: .stop)
  /// Resume's label.
  static let resume = ViewerControlLabel("Resume", symbol: .resume)

  /// The capsule, in its slot.
  var body: some View {
    LineSwap(label) { shown, isDrawn, isReady in
      Button {
        if isReady && isEnabled { send(isResume ? .resumeAgent : .stopAgent) }
      } label: {
        FooterLabel(
          label: shown,
          room: [Self.stop, Self.resume],
          symbolFont: Typography.symbol(.row),
          collapsesTitles: fold.isCircles,
          showsTitles: fold.showsTitles
        )
        .opacity(isDrawn ? 1 : 0)
      }
      .buttonStyle(.capsule(.secondary, size: .small, outline: fold.isCircles ? .circle : .symbolPill))
      .disabled(!isEnabled || !isReady)
      .help(help)
      .accessibilityLabel(agent?.accessibilityLabel ?? "")
      .actionSlot(agent != nil)
    }
  }

  /// Whether Resume stands in place of Stop.
  private var isResume: Bool {
    guard case .resume = agent else { return false }
    return true
  }

  /// The words and symbol whose action may be activated once their replacement settles.
  private var label: ViewerControlLabel {
    isResume ? Self.resume : Self.stop
  }

  /// Whether its action is available, independently of the label's replacement.
  private var isEnabled: Bool {
    guard agent != nil else { return false }
    guard case .resume(let isEnabled) = agent else { return true }
    return isEnabled
  }
}

#if DEBUG
  #Preview("Interactive") {
    @Previewable @State var stopped = false
    InteractivePreview {
      Toggle(String("Stopped"), isOn: $stopped)
    } content: {
      FooterAgentButton(agent: stopped ? .resume(isEnabled: true) : .stop, help: "") { _ in
        stopped.toggle()
      }
    }
  }
#endif
