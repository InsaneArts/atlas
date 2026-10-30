import SwiftUI

/// An alert as macOS 26 draws one: Atlas's icon, a bold title, what it means, and its capsule buttons, on an opaque
/// modal surface with a fine top highlight, lifted off the scrim by its shadow. Short labels sit
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
  @Environment(\.motionReduced) private var reduceMotion

  /// The icon, the title, the message and the buttons, on the modal surface.
  var body: some View {
    VStack(spacing: .zero) {
      AppIcon(tile: AlertCardLayout.icon)
        .frame(width: AlertCardLayout.icon, height: AlertCardLayout.icon)
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
    .frame(width: AlertCardLayout.width)
    .surface(.modal, in: shape)
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
    RoundedRectangle(cornerRadius: Radius.extraLarge, style: .continuous)
  }

  /// The surface's edge: a hairline all round, and a brighter one along the top that fades by the corners' end.
  private var edge: some View {
    ZStack {
      shape
        .strokeBorder(Color.strokeStrong, lineWidth: Stroke.hairline)
      shape
        .strokeBorder(AlertCardLayout.highlight, lineWidth: Stroke.hairline)
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
    labels.allSatisfy { $0.count <= AlertCardLayout.shortLabel } ? .sideBySide : .stacked
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
