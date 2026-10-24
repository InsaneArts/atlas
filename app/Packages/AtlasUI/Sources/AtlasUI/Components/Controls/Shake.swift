import SwiftUI

/// One shake from side to side each time `trigger` changes (v3 A4, V12, X12): left 5, right 5, left 3, right 3 and home
/// over ``Motion/Duration/shake``, for a command refused, such as an alert's Recover or the viewer's Take Control. Under
/// Reduce Motion it never moves; the words that come with the refusal say it.
struct Shake: ViewModifier {
  /// Counts the refusals seen: each change shakes once.
  let trigger: Int
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// The content, offset by the shake's keyframes each time the trigger changes.
  ///
  /// - Parameter content: The view that shakes.
  /// - Returns: The view, shaking.
  func body(content: Content) -> some View {
    content
      .keyframeAnimator(
        initialValue: CGFloat.zero,
        trigger: Self.trigger(trigger, reduceMotion: reduceMotion)
      ) { shaken, offset in
        shaken.offset(x: offset)
      } keyframes: { _ in
        // v3's `@keyframes shake`: left 5, right 5, left 3, right 3, home, in five even steps.
        KeyframeTrack {
          LinearKeyframe(-Layout.far, duration: Layout.step)
          LinearKeyframe(Layout.far, duration: Layout.step)
          LinearKeyframe(-Layout.near, duration: Layout.step)
          LinearKeyframe(Layout.near, duration: Layout.step)
          LinearKeyframe(.zero, duration: Layout.step)
        }
      }
  }

  /// What sets the shake off: the refusals counted, or nothing that ever changes under Reduce Motion.
  ///
  /// - Parameters:
  ///   - trigger: The refusals counted.
  ///   - reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: The value the keyframes follow.
  nonisolated static func trigger(_ trigger: Int, reduceMotion: Bool) -> Int {
    reduceMotion ? .zero : trigger
  }
}

/// v3's shake (`@keyframes shake`).
private enum Layout {
  /// 5: the first, furthest swing.
  static let far: CGFloat = 5
  /// 3: the second, nearer swing.
  static let near: CGFloat = 3
  /// One fifth of the shake.
  static let step = Motion.Duration.shake.seconds / 5
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
