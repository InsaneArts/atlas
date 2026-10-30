import SwiftUI

/// The hero over a screen that says how something ended: a green disc whose check draws itself, or an
/// orange-tinted disc with an orange mark. It plays once, as it arrives: the disc pops, then the check draws.
/// Under Reduce Motion the disc and check appear whole; a render draws them settled.
struct HeroDisc: View {
  /// How it ended.
  enum Kind: Sendable {
    /// It worked.
    case success
    /// Something needs the person.
    case problem
  }

  /// How it ended.
  let kind: Kind
  @State private var hasArrived = false
  @Environment(\.motionReduced) private var reduceMotion
  @Environment(\.animatesArrivals) private var animatesArrivals

  /// The disc and its mark.
  var body: some View {
    ZStack {
      Circle()
        .fill(kind == .success ? Color.success : Color.attentionFill)
        .scaleEffect(isSettled || reduceMotion ? 1 : HeroDiscLayout.arrivingScale)
      mark
    }
    .frame(width: HeroDiscLayout.side, height: HeroDiscLayout.side)
    .onAppear(perform: arrive)
    .accessibilityHidden(true)
  }

  /// The drawn check, or the problem's mark.
  @ViewBuilder private var mark: some View {
    switch kind {
    case .success:
      CheckStroke()
        .trim(from: 0, to: isSettled ? 1 : 0)
        .stroke(
          Color.onSuccess,
          style: StrokeStyle(lineWidth: HeroDiscLayout.checkWidth, lineCap: .round, lineJoin: .round)
        )
        .frame(width: HeroDiscLayout.check, height: HeroDiscLayout.check)
        .animation(Motion.animation(Motion.draw, reduceMotion: reduceMotion), value: hasArrived)
    case .problem:
      Image(systemName: SymbolName.exclamation.rawValue)
        .font(Typography.font(size: HeroDiscLayout.check, weight: .bold))
        .foregroundStyle(Color.attention)
    }
  }

  /// Whether the disc and its check show in full: once it has arrived, or at once where nothing plays.
  private var isSettled: Bool {
    hasArrived || !animatesArrivals
  }

  /// Plays the arrival once: the disc pops in, and the check draws after it on its own animation, ``Motion/draw``.
  /// Under Reduce Motion both appear whole.
  private func arrive() {
    guard !reduceMotion else {
      hasArrived = true
      return
    }
    withAnimation(Motion.pop) { hasArrived = true }
  }
}

#if DEBUG
  #Preview("Samples") {
    IndicatorSamples(.heroDisc)
  }

  #Preview("Interactive") {
    @Previewable @State var arrival = 0
    @Previewable @State var isSuccess = true
    InteractivePreview {
      Toggle(String("Success"), isOn: $isSuccess)
      Button(String("Arrive again")) { arrival += 1 }
    } content: {
      HeroDisc(kind: isSuccess ? .success : .problem)
        .id(arrival)
    }
  }
#endif
