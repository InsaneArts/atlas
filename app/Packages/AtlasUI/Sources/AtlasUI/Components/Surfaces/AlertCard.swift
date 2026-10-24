import SwiftUI

/// An alert as macOS 26 draws one: Atlas's icon, a bold title, what it means, and its capsule buttons, on a
/// near-opaque modal surface with a fine top highlight, opaque under Reduce Transparency. Short labels sit
/// side by side with the default on the right; a long one stacks them with the default on top. Return presses
/// the default and Escape the way out. Its command shows in place: a spinner beside the word of the button
/// that runs it while the others wait, a refusal as a problem line with one shake, and once done the done
/// title and message with one Done button in place of the rest, which Return presses; the card keeps its
/// height throughout. Words swap by opacity. The window lays it over its scrim.
struct AlertCard<ID: Hashable & Sendable>: View {
  /// How the buttons sit.
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
      AlertCardMessage(message: state.shownMessage, reserving: state.message, refusal: state.refusal)
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

  /// How the buttons sit: side by side when every label is ten characters or fewer, else stacked. The labels
  /// are the asked ones, so a running word, such as "Recovering…", never moves a button.
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

  /// Whether a change of refusal shakes the card: a new refusal does, unless Reduce Motion is on.
  ///
  /// - Parameters:
  ///   - refusal: The refusal now, or nil for none.
  ///   - reduceMotion: Whether Reduce Motion is on.
  /// - Returns: Whether to shake.
  nonisolated static func shakes(on refusal: String?, reduceMotion: Bool) -> Bool {
    refusal != nil && !reduceMotion
  }
}

/// The title, which swaps by opacity as it changes to the done title.
private struct AlertCardTitle: View {
  /// The title shown.
  let text: String

  var body: some View {
    Text(text)
      .textStyle(.body, weight: .bold)
      .multilineTextAlignment(.center)
      .fixedSize(horizontal: false, vertical: true)
      .swapsText(on: text)
      .accessibilityAddTraits(.isHeader)
  }
}

/// What the alert means, the done message in its place once done, or, once its command was refused, why, centred in
/// orange with no mark, as the card's own words are set. The asked message is laid out unseen beneath, so the card
/// keeps its height when a shorter done message takes its place. A refusal fades in as the slot eases to its height,
/// with no bounce.
private struct AlertCardMessage: View {
  /// What it means now: the asked message, or the done one.
  let message: String
  /// The asked message, whose height the slot keeps.
  let reserving: String
  /// Why its command was refused; nil until it was.
  let refusal: String?
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  var body: some View {
    ZStack(alignment: .top) {
      if let refusal {
        Text(refusal)
          .textStyle(.caption)
          .foregroundStyle(Color.textAttention)
          .multilineTextAlignment(.center)
          .fixedSize(horizontal: false, vertical: true)
          .frame(maxWidth: .infinity)
          .transition(.opacity)
      } else {
        ZStack(alignment: .top) {
          words(reserving)
            .hidden()
            .accessibilityHidden(true)
          words(message)
            .swapsText(on: message)
        }
        .transition(.opacity)
      }
    }
    .animation(Motion.animation(Motion.arrive, reduceMotion: reduceMotion, reduced: Motion.reduced), value: refusal)
  }

  /// The message's words, centred under the title.
  ///
  /// - Parameter text: The words.
  /// - Returns: The message.
  private func words(_ text: String) -> some View {
    Text(text)
      .textStyle(.caption)
      .foregroundStyle(Color.textOnGlass)
      .multilineTextAlignment(.center)
      .fixedSize(horizontal: false, vertical: true)
  }
}

/// The buttons, side by side or stacked, each 28 high and sharing the width. Once done they fade out under one
/// Done button that takes their place and the default's key, so the row keeps its height.
private struct AlertCardButtons<ID: Hashable & Sendable>: View {
  /// What the alert says and offers.
  let state: AlertCardState<ID>
  /// Receives the button the person presses.
  let send: @MainActor (ID) -> Void

  var body: some View {
    ZStack {
      asked
        .opacity(state.isDone ? 0 : 1)
        .allowsHitTesting(!state.isDone)
        .accessibilityHidden(state.isDone)
      if let done = state.doneButton {
        AlertCardDoneButton(button: done, send: send)
          .transition(.opacity)
      }
    }
    .animation(Motion.textSwap, value: state.isDone)
  }

  /// The asked buttons in their arrangement.
  @ViewBuilder private var asked: some View {
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

/// One asked button: its own word, after a spinner while its command runs, so its width never changes;
/// VoiceOver hears the running word. Every button waits while the command runs; the one running it stays whole.
private struct AlertCardButtonView<ID: Hashable & Sendable>: View {
  /// The button.
  let button: AlertCardButton<ID>
  /// What the alert says and offers.
  let state: AlertCardState<ID>
  /// Receives the button the person presses.
  let send: @MainActor (ID) -> Void

  var body: some View {
    Button(role: button.role.buttonRole) {
      send(button.id)
    } label: {
      HStack(spacing: Layout.markGap) {
        if state.isWorking(button) {
          Spinner(size: IconSize.xs, arc: button.role.spinnerArc)
            .transition(.opacity)
        }
        Text(state.label(of: button))
      }
      .animation(Motion.textSwap, value: state.isWorking(button))
    }
    .buttonStyle(.capsule(button.role.look, size: .compact, fill: true, isWorking: state.isWorking(button)))
    .keyboardShortcut(state.isBusy ? nil : button.role.shortcut)
    .disabled(state.isBusy)
    .accessibilityLabel(state.spokenLabel(of: button))
  }
}

/// The one button a done alert shows: a check, then its word, such as "Done", in the default's accent, which
/// Return presses to close the alert now.
private struct AlertCardDoneButton<ID: Hashable & Sendable>: View {
  /// The button, which sends the way out.
  let button: AlertCardButton<ID>
  /// Receives the button the person presses.
  let send: @MainActor (ID) -> Void

  var body: some View {
    Button {
      send(button.id)
    } label: {
      HStack(spacing: Layout.markGap) {
        Image(systemName: SymbolName.passed.rawValue)
          .symbolRenderingMode(.palette)
          .foregroundStyle(Color.accent, Color.onAccent)
          .font(Typography.symbol(.control, weight: .semibold))
          .accessibilityHidden(true)
        Text(button.title)
      }
    }
    .buttonStyle(.capsule(button.role.look, size: .compact, fill: true))
    .keyboardShortcut(.defaultAction)
  }
}

/// The alert card's own measures, at file scope because `AlertCard` is generic.
private enum Layout {
  /// 260: the alert's width.
  static let width: CGFloat = 260
  /// 52: the icon's tile; its canvas draws larger, past the frame.
  static let icon: CGFloat = 52
  /// 10: the longest label that still sits side by side.
  static let shortLabel = 10
  /// 7: between a button's mark and its word.
  static let markGap: CGFloat = 7
  /// The top highlight: the glass edge again, faded out by a fifth of the way down, where the corners end.
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
          .tag(AlertCardPhase.done(title: AlertCardSamples.ready, message: AlertCardSamples.workAgain, button: "Done"))
      }
      .fixedSize()
    } content: {
      AlertCard(state: state) { id in
        if id == "recover" { state.phase = .running(label: AlertCardSamples.recovering) }
      }
    }
  }
#endif
