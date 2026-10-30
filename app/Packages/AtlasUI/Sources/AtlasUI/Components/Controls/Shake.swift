import SwiftUI

/// One shake from side to side each time `trigger` changes: left 5, right 5, left 3, right 3 and home over
/// ``Motion/Duration/shake``, for a command refused, such as an alert's Recover or the viewer's Take Control.
/// Under Reduce Motion it never moves; the words that come with the refusal say it. The trigger is the
/// refusals alone, so turning Reduce Motion on or off never sets a shake off by itself.
struct Shake: ViewModifier {
  /// Counts the refusals seen: each change shakes once.
  let trigger: Int
  @Environment(\.motionReduced) private var reduceMotion

  /// The content, offset by the shake's keyframes each time the trigger changes.
  ///
  /// - Parameter content: The view that shakes.
  /// - Returns: The view, shaking.
  func body(content: Content) -> some View {
    content
      .keyframeAnimator(initialValue: CGFloat.zero, trigger: trigger) { shaken, offset in
        shaken.offset(x: offset)
      } keyframes: { _ in
        // The refusal shake: left 5, right 5, left 3, right 3, home, in five even steps; read as the refusal
        // comes, so under Reduce Motion every step stays home.
        let swings = Self.swings(reduceMotion: reduceMotion)
        KeyframeTrack {
          LinearKeyframe(swings[0], duration: Layout.step)
          LinearKeyframe(swings[1], duration: Layout.step)
          LinearKeyframe(swings[2], duration: Layout.step)
          LinearKeyframe(swings[3], duration: Layout.step)
          LinearKeyframe(swings[4], duration: Layout.step)
        }
      }
  }

  /// Where the shake swings in its five steps: left 5, right 5, left 3, right 3 and home; home throughout under Reduce
  /// Motion.
  ///
  /// - Parameter reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: The five offsets.
  nonisolated static func swings(reduceMotion: Bool) -> [CGFloat] {
    guard !reduceMotion else { return Array(repeating: .zero, count: Layout.steps) }
    return [-Layout.far, Layout.far, -Layout.near, Layout.near, .zero]
  }
}

/// The shake's horizontal offsets, shrinking as the motion settles.
private enum Layout {
  /// 5: the first, furthest swing.
  static let far: CGFloat = 5
  /// 3: the second, nearer swing.
  static let near: CGFloat = 3
  /// 5: the shake's steps.
  static let steps = 5
  /// One fifth of the shake.
  static let step = Motion.Duration.shake.seconds / Double(steps)
}

#if DEBUG
  #Preview("Interactive") {
    @Previewable @State var refusals = 0
    InteractivePreview {
      Button(String("Refuse")) { refusals += 1 }
    } content: {
      Button(String("Take Control")) {}
        .buttonStyle(.capsule(size: .small))
        .shakes(refusals)
    }
  }
#endif
