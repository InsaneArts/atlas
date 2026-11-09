import SwiftUI

/// A place for a control that shows only while its action applies, such as Keys while the person has control.
/// The row it sits in moves its controls on ``Motion/state`` as the slot comes or goes (``ActionSlotRow``), and the
/// control rides that move (``ActionSlotBeat``): coming, it shows only once its neighbours have nearly made room for
/// it; going, it has faded before they reach its place, so a sliding neighbour never passes over it. Under Reduce Motion
/// it only fades.
struct ActionSlot: ViewModifier {
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
        .transition(Motion.transition(Self.beats, reduceMotion: reduceMotion))
    }
  }

  /// The control coming in on the last of the row's move and going on the first of it.
  private static var beats: AnyTransition {
    .modifier(active: ActionSlotBeat(progress: .zero), identity: ActionSlotBeat(progress: 1))
  }

  /// How a row moves its controls as its slots' controls come or go.
  ///
  /// - Parameter reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: ``Motion/state``, or the short fade ``Motion/reduced`` under Reduce Motion.
  nonisolated static func animation(reduceMotion: Bool) -> Animation {
    Motion.animation(Motion.state, reduceMotion: reduceMotion, reduced: Motion.reduced)
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
