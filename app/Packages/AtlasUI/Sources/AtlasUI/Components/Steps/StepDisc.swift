import SwiftUI

/// A step's disc: its number in a faint ring while it waits, white on the accent while it is the one to
/// do, and a green disc with a drawn check once done. As a step finishes, the number shrinks away, the disc turns green
/// and pops, and the check draws; as a step lights up, it pops a little after the step before it finishes.
/// Under Reduce Motion the colours change and the check appears whole.
struct StepDisc: View {
  /// The step's number.
  let index: Int
  /// Where the step stands.
  let state: StepState
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// The ring or fill, the number and the check, popping as the state changes.
  var body: some View {
    ZStack {
      Circle()
        .filledOutline(fill, border: ring, lineWidth: Stroke.emphasis)
      Text(verbatim: "\(index)")
        .textStyle(.caption, weight: .semibold)
        .foregroundStyle(numberColor)
        .scaleEffect(numberScale)
        .opacity(isDone ? 0 : 1)
        .animation(Motion.animation(Motion.stepOut, reduceMotion: reduceMotion, reduced: Motion.reduced), value: isDone)
      CheckStroke()
        .trim(from: 0, to: isDone ? 1 : 0)
        .stroke(Color.onSuccess, style: StrokeStyle(lineWidth: Stroke.thick, lineCap: .round, lineJoin: .round))
        .frame(width: Layout.check, height: Layout.check)
        .animation(Motion.animation(Motion.draw, reduceMotion: reduceMotion), value: isDone)
    }
    .frame(width: Self.side, height: Self.side)
    .keyframeAnimator(initialValue: CGFloat(1), trigger: state) { disc, scale in
      disc.scaleEffect(scale)
    } keyframes: { _ in
      Self.pop(hold: hold, from: popFrom)
    }
    .accessibilityHidden(true)
  }

  /// The disc's pop as its state changes: it holds whole for `hold`, then jumps to `from` and springs back whole on
  /// ``Motion/Springs/pop``.
  ///
  /// - Parameters:
  ///   - hold: How long it stays whole first, in seconds; zero for none.
  ///   - from: The scale it jumps to before springing back.
  /// - Returns: The keyframes.
  @KeyframesBuilder<CGFloat>
  nonisolated static func pop(hold: TimeInterval, from: CGFloat) -> some Keyframes<CGFloat> {
    // The jump is a move, never a zero-length linear keyframe: that divides by zero, and the NaN scale it gives makes
    // AppKit abort the app in NSView.setFrameTransform.
    KeyframeTrack {
      if hold > 0 {
        LinearKeyframe(1, duration: hold)
      } else {
        MoveKeyframe(1)
      }
      MoveKeyframe(from)
      SpringKeyframe(1, spring: Motion.Springs.pop)
    }
  }

  /// 22: the disc's side.
  nonisolated static let side: CGFloat = 22

  /// Whether the step is done.
  private var isDone: Bool {
    state == .done
  }

  /// The number's scale: whole, shrinking away as the step finishes; under Reduce Motion it only fades.
  private var numberScale: CGFloat {
    guard isDone, !reduceMotion else { return 1 }
    return Layout.numberScale
  }

  /// The disc's fill: green once done, the accent while current, none while it waits.
  private var fill: Color {
    switch state {
    case .done: .success
    case .current: .accent
    case .waiting: .clear
    }
  }

  /// The faint ring a waiting step's number sits in; none on a filled disc.
  private var ring: Color {
    state == .waiting ? .strokeStrong : .clear
  }

  /// The number's colour: white on the accent, tertiary in the faint ring.
  private var numberColor: Color {
    state == .current ? .onAccent : .textTertiary
  }

  /// The scale the disc pops from as its state changes: 0.8 as it finishes, 0.9 as it lights up, none as it goes back
  /// to waiting or under Reduce Motion.
  private var popFrom: CGFloat {
    guard !reduceMotion else { return 1 }
    switch state {
    case .done: return Layout.finishScale
    case .current: return Layout.lightScale
    case .waiting: return 1
    }
  }

  /// How long the disc holds before it pops: as long as a step lighting up waits for the one before it.
  private var hold: TimeInterval {
    guard state == .current, !reduceMotion else { return .zero }
    return Motion.Duration.step.seconds
  }
}

/// The step disc's own measures.
private enum Layout {
  /// 11: the drawn check's frame.
  static let check: CGFloat = 11
  /// 0.5: the size the number shrinks to as it leaves.
  static let numberScale: CGFloat = 0.5
  /// 0.8: the size a finishing step's disc pops from.
  static let finishScale: CGFloat = 0.8
  /// 0.9: the size a step lighting up pops from.
  static let lightScale: CGFloat = 0.9
}

#if DEBUG
  #Preview("Samples") {
    HStack(spacing: Spacing.l) {
      StepDisc(index: 1, state: .done)
      StepDisc(index: 2, state: .current)
      StepDisc(index: 3, state: .waiting)
    }
    .sampleSurface()
  }

  #Preview("Interactive") {
    @Previewable @State var state = StepState.waiting
    InteractivePreview {
      Picker(String("State"), selection: $state) {
        Text(verbatim: "Waiting")
          .tag(StepState.waiting)
        Text(verbatim: "Current")
          .tag(StepState.current)
        Text(verbatim: "Done")
          .tag(StepState.done)
      }
      .pickerStyle(.segmented)
      .fixedSize()
    } content: {
      StepDisc(index: 2, state: state)
    }
  }
#endif
