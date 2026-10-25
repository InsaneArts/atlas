import SwiftUI

/// The small mark before a status sentence or a row's word: a spinner while work runs, an orange round "!"
/// for a problem, a green check once done. The spinner waits ``Motion/appear``'s delay before it shows. When
/// the outcome changes under an animation, the old mark shrinks out and the new one pops in; under Reduce
/// Motion they crossfade.
struct OutcomeMark: View {
  /// What the mark says.
  let kind: StatusLine.Kind
  /// The mark's side, in points.
  var side = StatusLine.markSide
  @Environment(\.accessibilityReduceMotion) private var reduceMotion
  @Environment(\.marksSwapAtOnce) private var swapsAtOnce
  @Environment(\.marksFade) private var marksFade

  /// The spinner, the round "!" or the check, one at a time, landing in place of the last.
  var body: some View {
    ZStack {
      mark
        .id(kind)
        .transition(marksFade ? .opacity : Motion.transition(Self.landing, reduceMotion: reduceMotion))
    }
    .frame(width: side, height: side)
    .accessibilityHidden(true)
  }

  /// The mark for the outcome now.
  @ViewBuilder private var mark: some View {
    switch kind {
    case .working:
      Spinner(size: side)
        .modifier(DelayedAppearance(holds: !swapsAtOnce))
    case .problem:
      Image(systemName: SymbolName.attention.rawValue)
        .font(Typography.font(size: side))
        .foregroundStyle(Self.color(kind))
    case .done:
      Image(systemName: SymbolName.passed.rawValue)
        .font(Typography.font(size: side))
        .foregroundStyle(Self.color(kind))
    }
  }

  /// How one mark gives way to the next: the old one shrinks out as it fades, and the new one pops in from
  /// the same small size.
  private static var landing: AnyTransition {
    .asymmetric(
      insertion: .scale(scale: Layout.landingScale)
        .combined(with: .opacity)
        .animation(Motion.pop),
      removal: .scale(scale: Layout.landingScale)
        .combined(with: .opacity)
        .animation(Motion.leave)
    )
  }

  /// The mark's colour: system orange for a problem, system green once done, the accent of the spinner's arc while
  /// work runs.
  ///
  /// - Parameter kind: What the mark says.
  /// - Returns: The colour.
  nonisolated static func color(_ kind: StatusLine.Kind) -> Color {
    switch kind {
    case .working: .accent
    case .problem: .attention
    case .done: .success
    }
  }
}

/// The mark's scale while it appears or gives way to another.
private enum Layout {
  /// 0.6: the size a mark shrinks to as it leaves, and pops in from.
  static let landingScale: CGFloat = 0.6
}

#if DEBUG
  #Preview("Samples") {
    StatusLineSamples(.marks)
  }

  #Preview("Interactive") {
    @Previewable @State var isDone = false
    InteractivePreview {
      Toggle(String("Done"), isOn: $isDone.animation(Motion.state))
    } content: {
      OutcomeMark(kind: isDone ? .done : .working, side: IconSize.s)
    }
  }
#endif
