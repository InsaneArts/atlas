import SwiftUI

/// A place for a control that shows only while its action applies, such as Keys while the person has control.
/// The control grows in from its leading edge and shrinks away there, in the list's three beats, so a neighbour
/// sliding never passes over it: a control going fades first (``Motion/swapOut``), the row then moves its neighbours
/// to their places (``ActionSlotRow``, on ``Motion/listMove``), and a control coming fades in once they have made room
/// for it (``Motion/listIn``). Under Reduce Motion it only fades.
struct ActionSlot: ViewModifier {
  /// Close to full size so the control reads as arriving rather than popping.
  private static let arrivingScale: CGFloat = 0.94
  /// Whether the action applies, so the control shows.
  let isShown: Bool
  @Environment(\.motionReduced) private var reduceMotion

  /// The control while it applies, nothing otherwise.
  ///
  /// - Parameter content: The control.
  /// - Returns: The control in its slot.
  func body(content: Content) -> some View {
    if isShown {
      content
        .transition(Motion.transition(beats, reduceMotion: reduceMotion))
    }
  }

  /// The control growing from its leading edge as it fades in once its neighbours have made room, and shrinking back
  /// as it fades out before they close the gap.
  private var beats: AnyTransition {
    .asymmetric(
      insertion: growing.animation(Motion.listIn),
      removal: growing.animation(Motion.swapOut)
    )
  }

  /// The control growing from its leading edge as it fades in, and shrinking back as it fades out.
  private var growing: AnyTransition {
    AnyTransition.scale(scale: Self.arrivingScale, anchor: .leading)
      .combined(with: .opacity)
  }

  /// How a row moves its controls as its slots' controls come or go.
  ///
  /// - Parameter reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: ``Motion/listMove``, once a control going has faded, or the short fade ``Motion/reduced`` under
  ///   Reduce Motion.
  nonisolated static func animation(reduceMotion: Bool) -> Animation {
    Motion.animation(Motion.listMove, reduceMotion: reduceMotion, reduced: Motion.reduced)
  }
}

#if DEBUG
  #Preview("Samples") {
    ActionSlotSamples(isInControl: true)
  }

  #Preview("Interactive") {
    @Previewable @State var isInControl = false
    InteractivePreview {
      Toggle(String("In control"), isOn: $isInControl)
    } content: {
      ActionSlotSamples(isInControl: isInControl)
    }
  }
#endif
