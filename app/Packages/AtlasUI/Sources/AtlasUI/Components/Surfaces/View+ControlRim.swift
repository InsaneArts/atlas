import SwiftUI

extension View {
  /// Edges a picture: a quiet hairline, or the blue rim and glow of control.
  ///
  /// - Parameters:
  ///   - isInControl: Whether the person holds control.
  ///   - radius: The edge's corners.
  /// - Returns: The picture with its edge.
  func controlRim(_ isInControl: Bool, radius: CGFloat) -> some View {
    modifier(ControlRim(isInControl: isInControl, radius: radius))
  }
}

#if DEBUG
  #Preview("Samples") {
    ModifierSamples(.controlRim)
  }
#endif
