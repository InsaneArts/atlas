import SwiftUI

/// A place for a control that shows only while its action applies, such as Keys while the person has control.
/// The control grows in from its leading edge and shrinks away there; the row it sits in carries the
/// animation (``ActionSlotRow``), so the controls beside it slide to close the gap as the slot goes from
/// nothing to its width and back. Under Reduce Motion it only fades.
struct ActionSlot: ViewModifier {
  /// Whether the action applies, so the control shows.
  let isShown: Bool
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// The control while it applies, nothing otherwise.
  ///
  /// - Parameter content: The control.
  /// - Returns: The control in its slot.
  func body(content: Content) -> some View {
    if isShown {
      content
        .transition(Motion.transition(growing, reduceMotion: reduceMotion))
    }
  }

  /// The control growing from its leading edge as it fades in, and shrinking back as it fades out.
  private var growing: AnyTransition {
    AnyTransition.scale(scale: Layout.scale, anchor: .leading)
      .combined(with: .opacity)
  }

  /// How a row moves as its slots' controls come or go.
  ///
  /// - Parameters:
  ///   - isShown: Whether the controls are showing now.
  ///   - reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: ``Motion/arrive`` as they come, ``Motion/leave`` as they go, and the short fade ``Motion/reduced``
  ///   under Reduce Motion.
  nonisolated static func animation(isShown: Bool, reduceMotion: Bool) -> Animation {
    Motion.animation(isShown ? Motion.arrive : Motion.leave, reduceMotion: reduceMotion, reduced: Motion.reduced)
  }
}

/// The slot's own measures.
private enum Layout {
  /// 0.94: the scale a control grows from and shrinks to, close to its own size, so it reads as arriving rather
  /// than popping.
  static let scale: CGFloat = 0.94
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
