import SwiftUI

/// Why Take Control was just refused, on glass at the top of the stage: an orange mark and the reason in orange. It
/// comes down a few points as it fades in and fades out quicker (``transition(reduceMotion:)``); the controller takes
/// it away after 2.4 seconds.
struct ViewerToastChip: View {
  /// The reason.
  let text: String

  /// The mark and the reason on glass.
  var body: some View {
    GlassChip {
      Image(systemName: SymbolName.attention.rawValue)
        .font(Typography.symbol(.row, weight: .medium))
        .foregroundStyle(Color.attention)
        .accessibilityHidden(true)
      Text(text)
        .foregroundStyle(Color.textAttention)
    }
  }

  /// How the toast comes and goes: in from ``Layout/rise`` above its place as it fades in, on ``Motion/arrive``; out
  /// by fading on ``Motion/dismiss``; a plain fade both ways under Reduce Motion.
  ///
  /// - Parameter reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: The transition.
  static func transition(reduceMotion: Bool) -> AnyTransition {
    guard !reduceMotion else {
      return AnyTransition.opacity
        .animation(Motion.reduced)
    }
    return .asymmetric(
      insertion: AnyTransition.opacity
        .combined(with: .offset(y: -Layout.rise))
        .animation(Motion.arrive),
      removal: AnyTransition.opacity
        .animation(Motion.dismiss)
    )
  }
}

/// The toast's own measure.
private enum Layout {
  /// 8: how far above its place the toast starts as it fades in.
  static let rise: CGFloat = 8
}

#if DEBUG
  #Preview("Mid-action") {
    ViewerToastChip(text: "The agent didn't finish its step. Try again.")
      .sampleSurface()
  }
#endif
