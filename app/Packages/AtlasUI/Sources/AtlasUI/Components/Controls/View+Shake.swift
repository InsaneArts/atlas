import SwiftUI

extension View {
  /// Shakes once from side to side each time `trigger` changes, never under Reduce Motion (``Shake``).
  ///
  /// - Parameter trigger: Counts the refusals seen.
  /// - Returns: The view, shaking on each.
  func shakes(_ trigger: Int) -> some View {
    modifier(Shake(trigger: trigger))
  }
}

#if DEBUG
  #Preview("Interactive") {
    @Previewable @State var refusals = 0
    InteractivePreview {
      Button(String("Refuse")) { refusals += 1 }
    } content: {
      AlertCardSamples()
        .shakes(refusals)
    }
  }
#endif
