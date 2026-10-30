import SwiftUI

/// The footer's main capsule in its slot: Take Control with its hand, Connecting…, Recover…, or Give Back in the accent
/// while this window holds control. It stands at one width through all of them, the widest label's
/// (``FooterLabel``), its trailing edge in its place, so a press never moves it: only its words swap, with no
/// overlap, the hand going with the words it leads, and its fill eases from one look to the next on
/// ``Motion/crossfade``. A refusal moves nothing; its reason takes the footer's line. In a narrow card, a threshold
/// the window's width crosses and never a press, a label with a symbol, such as Take Control, gives way to the symbol
/// alone (``FooterFold``) while one with none, such as Give Back, keeps its words, the capsule keeping the widest of
/// those.
struct ViewerMainButton: View {
  /// The button; nil while the stage offers what to do instead.
  let control: ViewerControlButton?
  /// How the footer draws in a narrow card.
  var fold = FooterFold(.full)
  /// Receives the press.
  let send: @MainActor (ViewerEvent) -> Void
  @Environment(\.motionReduced) private var reduceMotion

  /// The capsule, its symbol and title.
  var body: some View {
    LineSwap(control?.label) { shown, isDrawn, isReady in
      Button {
        if isReady, let control, control.isEnabled { send(control.event) }
      } label: {
        FooterLabel(
          label: shown,
          room: control?.room ?? [],
          symbolFont: Typography.symbol(.glyph),
          collapsesTitles: fold.isCircles,
          showsTitles: fold.showsTitles
        )
        .opacity(isDrawn ? 1 : 0)
      }
      .buttonStyle(.capsule(look, size: .small, outline: outline))
      .disabled(control?.isEnabled != true || !isReady)
      .animation(Self.fillMotion(reduceMotion: reduceMotion), value: control?.look)
      .animation(Self.fillMotion(reduceMotion: reduceMotion), value: control?.isEnabled)
      .help(control?.help ?? "")
      .accessibilityLabel(control?.accessibilityLabel ?? "")
      .actionSlot(control != nil)
    }
  }

  /// How the fill moves from one look to the next: a short ease, ``Motion/crossfade``, or ``Motion/reduced`` under
  /// Reduce Motion; nothing about the capsule's size moves with it.
  ///
  /// - Parameter reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: The animation.
  nonisolated static func fillMotion(reduceMotion: Bool) -> Animation {
    Motion.animation(Motion.crossfade, reduceMotion: reduceMotion, reduced: Motion.reduced)
  }

  /// The shape round its label: one capsule, a little tighter before a symbol, for every label it takes, so their
  /// widths match; a circle only when every one of them stands as a symbol alone in a narrow card.
  private var outline: CapsuleButtonStyle.Outline {
    let room = control?.room ?? []
    if fold.isCircles, !room.isEmpty, room.allSatisfy({ $0.symbol != nil }) { return .circle }
    return room.contains { $0.symbol != nil } ? .symbolPill : .pill
  }

  /// How the capsule is painted: the text's colour, or the accent while this window holds control.
  private var look: CapsuleButtonStyle.Look {
    control?.look == .accent ? .accent : .primary
  }
}

#if DEBUG
  #Preview("Take Control and Give Back") {
    HStack(spacing: Spacing.s) {
      ViewerMainButton(control: PreviewStates.viewerWorking.footer.controls.control) { _ in }
      ViewerMainButton(control: PreviewStates.viewerControl.footer.controls.control) { _ in }
    }
    .padding()
    .sampleSurface()
  }
#endif
