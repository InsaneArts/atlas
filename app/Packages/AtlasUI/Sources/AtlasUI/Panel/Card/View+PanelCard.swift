import SwiftUI

extension View {
  /// Draws the panel as the board does where no popover holds it, as in a design render or a preview (v3 P3): the
  /// glass under the panel's tinted ground, opaque under Reduce Transparency, its corners, a fine edge with a brighter
  /// 0.5-point highlight along the top, and its shadow.
  ///
  /// - Returns: The panel on its card.
  public func panelCard() -> some View {
    let shape = RoundedRectangle(cornerRadius: PanelLayout.corner)
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
        Color.clear
          .glass(.regular, in: shape)
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
