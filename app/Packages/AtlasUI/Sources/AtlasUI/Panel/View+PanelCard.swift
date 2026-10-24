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

/// The card's top highlight (v3 `.panel2`'s `inset 0 .5px 0`, R3): the light edge along the top, fading out where
/// the corners end.
private enum PanelCardHighlight {
  /// The highlight's paint, from the top edge to a tenth of the way down.
  static var gradient: LinearGradient {
    LinearGradient(
      colors: [Color.strokeHighlight, .clear],
      startPoint: .top,
      endPoint: UnitPoint(x: 0.5, y: 0.1)
    )
  }
}

/// Where the card's shadow may show: everywhere but under the card, so the glass never darkens over its own shadow.
private struct PanelCardShadowMask: View {
  /// The card's outline.
  let shape: RoundedRectangle

  var body: some View {
    Rectangle()
      .padding(-PanelLayout.shadowRoom)
      .overlay {
        shape
          .blendMode(.destinationOut)
      }
      .compositingGroup()
  }
}

#if DEBUG
  #Preview("Samples") {
    ModifierSamples(.panelCard)
  }
#endif
