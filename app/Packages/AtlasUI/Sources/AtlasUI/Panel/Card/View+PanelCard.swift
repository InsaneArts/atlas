import SwiftUI

extension View {
  /// Draws the panel's surface where no popover holds it, as in a design render or a preview: its corners round the
  /// panel's own opaque ground, a fine edge with a brighter 0.5-point highlight along the top, and its shadow.
  ///
  /// - Returns: The panel on its card.
  public func panelCard() -> some View {
    let shape = RoundedRectangle(cornerRadius: PanelLayout.corner, style: .continuous)
    return
      clipShape(shape)
      .overlay(
        ZStack {
          shape
            .strokeBorder(Color.strokeStrong, lineWidth: Stroke.hairline)
          shape
            .strokeBorder(PanelCardHighlight.gradient, lineWidth: Stroke.hairline)
        }
        .allowsHitTesting(false)
      )
      .background(
        shape
          .fill(PanelLayout.ground)
          .shadow(color: PanelLayout.shadowColor, radius: PanelLayout.shadowRadius, y: PanelLayout.shadowDrop)
          .mask(PanelCardShadowMask(shape: shape))
      )
  }
}

#if DEBUG
  #Preview("Samples") {
    ModifierSamples(.panelCard)
  }
#endif
