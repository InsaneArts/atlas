import SwiftUI

/// An alert as macOS 26 draws one (v3 A1–A5): Atlas's icon, a bold title, what it means, and its capsule buttons, on
/// thick glass with a fine top highlight, opaque under Reduce Transparency. Short labels sit side by side with the
/// default on the right; a long one stacks them with the default on top. Return presses the default and Escape the
/// way out. Its command shows in place: a spinner in the button that runs it while the others wait, a refusal as a
/// problem line with one shake, and a check with the done title before it closes. The window lays it over its scrim.
struct AlertCard<ID: Hashable & Sendable>: View {
  /// How the buttons sit (v3 A2).
  enum Arrangement: Equatable, Sendable {
    /// In one row, sharing it, the default on the right.
    case sideBySide
    /// One under another, the default on top.
    case stacked
  }

  /// What the alert says and offers.
  let state: AlertCardState<ID>
  /// Receives the button the person presses.
  let send: @MainActor (ID) -> Void
  /// Counts the refusals seen, so each one shakes the card once.
  @State private var refusals = 0
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// The icon, the title, the message and the buttons, on the glass.
  var body: some View {
    VStack(spacing: .zero) {
      AppIcon(tile: Layout.icon)
        .frame(width: Layout.icon, height: Layout.icon)
      AlertCardTitle(text: state.shownTitle)
        .padding(.top, Spacing.sPlus)
      AlertCardMessage(message: state.shownMessage, refusal: state.refusal)
        .padding(.top, Spacing.xsPlus)
      AlertCardButtons(state: state, send: send)
        .padding(.top, Spacing.l)
    }
    .background { escapeKey }
    .padding(.top, Spacing.xl)
    .padding([.horizontal, .bottom], Spacing.l)
    .frame(width: Layout.width)
    .glass(.thick, in: shape)
    .overlay(edge)
    .background { CutoutShadow(shape: shape, level: .modal) }
    .shakes(refusals)
    .onChange(of: state.refusal) { _, refusal in
      if Self.shakes(on: refusal, reduceMotion: reduceMotion) { refusals += 1 }
    }
    .onExitCommand { if let escape = state.escape { send(escape) } }
    .accessibilityElement(children: .contain)
    .accessibilityAddTraits(.isModal)
  }

  /// The card's outline.
  private var shape: RoundedRectangle {
    RoundedRectangle(cornerRadius: Radius.extraLarge)
  }

  /// The glass's edge: a hairline all round, and a brighter one along the top that fades by the corners' end.
  private var edge: some View {
    ZStack {
      shape
        .strokeBorder(Color.strokeStrong, lineWidth: Stroke.hairline)
      shape
        .strokeBorder(Layout.highlight, lineWidth: Stroke.hairline)
    }
    .allowsHitTesting(false)
  }

  /// Escape for an alert whose way out takes Return: a key of its own, which works without a focused view, as
  /// `onExitCommand` alone does not unless Full Keyboard Access is on.
  @ViewBuilder private var escapeKey: some View {
    if state.escapeNeedsKey {
      Button(String()) { if let escape = state.escape { send(escape) } }
        .keyboardShortcut(.cancelAction)
        .frame(width: .zero, height: .zero)
        .opacity(.zero)
        .accessibilityHidden(true)
    }
  }

  /// How the buttons sit: side by side when every label is ten characters or fewer, else stacked (v3 A2). The
  /// labels are the asked ones, so a running word, such as "Recovering…", never moves a button.
  ///
  /// - Parameter labels: The buttons' words as first asked.
  /// - Returns: The arrangement.
  nonisolated static func arrangement(labels: [String]) -> Arrangement {
    labels.allSatisfy { $0.count <= Layout.shortLabel } ? .sideBySide : .stacked
  }

  /// The buttons in the order they are drawn: the state lists the default first, which is the top of a stack; side
  /// by side it goes on the right.
  ///
  /// - Parameters:
  ///   - buttons: The buttons, the default first.
  ///   - arrangement: How they sit.
  /// - Returns: The buttons, leading or top first.
  nonisolated static func ordered(
    _ buttons: [AlertCardButton<ID>],
    in arrangement: Arrangement
  ) -> [AlertCardButton<ID>] {
    arrangement == .sideBySide ? buttons.reversed() : buttons
  }

  /// Whether a change of refusal shakes the card: a new refusal does, unless Reduce Motion is on (v3 A4).
  ///
  /// - Parameters:
  ///   - refusal: The refusal now, or nil for none.
  ///   - reduceMotion: Whether Reduce Motion is on.
  /// - Returns: Whether to shake.
  nonisolated static func shakes(on refusal: String?, reduceMotion: Bool) -> Bool {
    refusal != nil && !reduceMotion
  }
}

/// The title, which blur-replaces as it changes to the done title (v3 A5), or fades under Reduce Motion.
private struct AlertCardTitle: View {
  /// The title shown.
  let text: String
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  var body: some View {
    ZStack {
      Text(text)
        .textStyle(.body, weight: .bold)
        .multilineTextAlignment(.center)
        .fixedSize(horizontal: false, vertical: true)
        .id(text)
        .transition(Motion.transition(AnyTransition(.blurReplace), reduceMotion: reduceMotion))
        .accessibilityAddTraits(.isHeader)
    }
    .animation(Motion.animation(Motion.state, reduceMotion: reduceMotion, reduced: Motion.reduced), value: text)
  }
}

/// What the alert means, or, once its command was refused, why, as a problem line (v3 A4); nothing once done.
private struct AlertCardMessage: View {
  /// What it means; nil once done.
  let message: String?
  /// Why its command was refused; nil until it was.
  let refusal: String?
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  var body: some View {
    ZStack {
      if let refusal {
        StatusLine(kind: .problem, text: refusal)
          .transition(Motion.transition(AnyTransition(.blurReplace), reduceMotion: reduceMotion))
      } else if let message {
        Text(message)
          .textStyle(.caption)
          .foregroundStyle(Color.textOnGlass)
          .multilineTextAlignment(.center)
          .fixedSize(horizontal: false, vertical: true)
          .transition(Motion.transition(AnyTransition(.blurReplace), reduceMotion: reduceMotion))
      }
    }
    .animation(Motion.animation(Motion.arrive, reduceMotion: reduceMotion, reduced: Motion.reduced), value: refusal)
  }
}

/// The buttons, side by side or stacked (v3 A2), each 28 high and sharing the width.
private struct AlertCardButtons<ID: Hashable & Sendable>: View {
  /// What the alert says and offers.
  let state: AlertCardState<ID>
  /// Receives the button the person presses.
  let send: @MainActor (ID) -> Void

  var body: some View {
    switch arrangement {
    case .sideBySide:
      HStack(spacing: Spacing.s) { buttons }
    case .stacked:
      VStack(spacing: Spacing.s) { buttons }
    }
  }

  /// How the buttons sit.
  private var arrangement: AlertCard<ID>.Arrangement {
    AlertCard<ID>.arrangement(labels: state.buttons.map(\.title))
  }

  /// Each button in drawing order.
  private var buttons: some View {
    ForEach(AlertCard<ID>.ordered(state.buttons, in: arrangement)) { button in
      AlertCardButtonView(button: button, state: state, send: send)
    }
  }
}

/// One button: its word, after a spinner while its command runs or a check once it worked. Every button waits while
/// the command runs; the one running it stays whole.
private struct AlertCardButtonView<ID: Hashable & Sendable>: View {
  /// The button.
  let button: AlertCardButton<ID>
  /// What the alert says and offers.
  let state: AlertCardState<ID>
  /// Receives the button the person presses.
  let send: @MainActor (ID) -> Void
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  var body: some View {
    Button(role: button.role.buttonRole) {
      send(button.id)
    } label: {
      HStack(spacing: Layout.markGap) {
        if state.isWorking(button) {
          AlertCardButtonMark(done: !isRunning, arc: button.role.spinnerArc)
            .transition(Motion.transition(.scale(scale: Layout.markArrival), reduceMotion: reduceMotion))
        }
        Text(state.label(of: button))
      }
      .animation(Motion.animation(Motion.pop, reduceMotion: reduceMotion, reduced: Motion.reduced), value: state.phase)
    }
    .buttonStyle(.capsule(button.role.look, size: .compact, fill: true, isWorking: state.isWorking(button)))
    .keyboardShortcut(state.isBusy ? nil : button.role.shortcut)
    .disabled(state.isBusy)
  }

  /// Whether its command still runs, before the check.
  private var isRunning: Bool {
    if case .running = state.phase { return true }
    return false
  }
}

/// The mark inside a working button: a spinner while it runs, then a check that pops in (v3 A3, A5).
private struct AlertCardButtonMark: View {
  /// Whether the command worked, so the check shows.
  let done: Bool
  /// The spinner's arc colour on this button.
  let arc: Color
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  var body: some View {
    ZStack {
      if done {
        Image(systemName: SymbolName.passed.rawValue)
          .symbolRenderingMode(.palette)
          .foregroundStyle(Color.accent, Color.onAccent)
          .font(Typography.symbol(.control, weight: .semibold))
          .transition(Motion.transition(.scale(scale: Layout.markArrival), reduceMotion: reduceMotion))
      } else {
        Spinner(size: IconSize.xs, arc: arc)
      }
    }
    .frame(width: width, height: IconSize.xsPlus)
    .accessibilityHidden(true)
  }

  /// The mark's width: the check's 14, the spinner's 12 (v3 `.acap .ckm`, `.acap .spin`).
  private var width: CGFloat {
    done ? IconSize.xsPlus : IconSize.xs
  }
}

/// At file scope because `AlertCard` is generic: v3's alert (`.alert2`, `.acap`).
private enum Layout {
  /// 260: the alert's width.
  static let width: CGFloat = 260
  /// 52: the icon's tile (v3 B1's alert tile); its canvas draws larger, past the frame.
  static let icon: CGFloat = 52
  /// 10: the longest label that still sits side by side.
  static let shortLabel = 10
  /// 7: between a button's mark and its word (v3 `.acap .spin`'s margin).
  static let markGap: CGFloat = 7
  /// 0.5: a mark pops in from half its size (v3 `runAlert`).
  static let markArrival: CGFloat = 0.5
  /// The top highlight (v3 `inset 0 .5px 0 rgb(255 255 255 / .16)`): the glass edge again, faded out by a fifth of
  /// the way down, where the corners end.
  static var highlight: LinearGradient {
    LinearGradient(
      colors: [Color.strokeStrong, .clear],
      startPoint: .top,
      endPoint: UnitPoint(x: 0.5, y: 0.2)
    )
  }
}

#if DEBUG
  #Preview("Samples") {
    AlertCardSamples()
  }

  #Preview("Interactive") {
    @Previewable @State var state = AlertCardSamples.recover
    InteractivePreview {
      Picker(String("Phase"), selection: $state.phase) {
        Text(verbatim: "Asking")
          .tag(AlertCardPhase.asking)
        Text(verbatim: "Running")
          .tag(AlertCardPhase.running(label: AlertCardSamples.recovering))
        Text(verbatim: "Refused")
          .tag(AlertCardPhase.refused(message: AlertCardSamples.refusal))
        Text(verbatim: "Done")
          .tag(AlertCardPhase.done(title: AlertCardSamples.recovered))
      }
      .fixedSize()
    } content: {
      AlertCard(state: state) { id in
        if id == "recover" { state.phase = .running(label: AlertCardSamples.recovering) }
      }
    }
  }
#endif
