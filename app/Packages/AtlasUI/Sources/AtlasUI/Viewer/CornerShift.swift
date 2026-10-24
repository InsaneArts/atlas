import SwiftUI

/// How the mini screen comes and goes in its corner (v3 V16): growing out of the stage's picture as the Screen is
/// left, it arrives whole, since it is that picture moving; arriving on its own, as from Screenshots or with the first
/// picture, it fades in from a little smaller on ``Motion/arrive``; it always leaves by fading, quickly, over the
/// picture it turns back into. Under Reduce Motion it only crossfades, over 0.15 seconds.
struct CornerShift: ViewModifier {
  /// Whether the mini screen comes in, rather than goes.
  let isArriving: Bool
  /// Whether it is at its transition's far end: not yet in, or already gone.
  let isAway: Bool
  /// Whether it arrives as the stage's picture moving into the corner.
  let growsFromStage: Bool
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// The mini screen, faded and scaled while it is away.
  func body(content: Content) -> some View {
    content
      .opacity(ScreenZoom.opacity(isArriving: isArriving, isAway: isAway, zooms: growsFromStage))
      .scaleEffect(
        ViewerLayout.miniScale(
          isComing: isArriving && isAway,
          growsFromStage: growsFromStage,
          reduceMotion: reduceMotion
        )
      )
  }

  /// The mini screen's transition: in on ``Motion/arrive``, out on ``Motion/crossfadeOut``; both on
  /// ``Motion/crossfadeReduced`` under Reduce Motion. Its arrival is chosen as it is inserted, in the switch that
  /// brings it, so it never waits on the switch to settle.
  ///
  /// - Parameters:
  ///   - growsFromStage: Whether it arrives as the stage's picture moving into the corner.
  ///   - reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: The transition.
  static func transition(growsFromStage: Bool, reduceMotion: Bool) -> AnyTransition {
    .asymmetric(
      insertion: AnyTransition.modifier(
        active: CornerShift(isArriving: true, isAway: true, growsFromStage: growsFromStage),
        identity: CornerShift(isArriving: true, isAway: false, growsFromStage: growsFromStage)
      )
      .animation(Motion.animation(Motion.arrive, reduceMotion: reduceMotion, reduced: Motion.crossfadeReduced)),
      removal: AnyTransition.modifier(
        active: CornerShift(isArriving: false, isAway: true, growsFromStage: false),
        identity: CornerShift(isArriving: false, isAway: false, growsFromStage: false)
      )
      .animation(Motion.animation(Motion.crossfadeOut, reduceMotion: reduceMotion, reduced: Motion.crossfadeReduced))
    )
  }
}
