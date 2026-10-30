import SwiftUI

/// Where the panel card's shadow may show: everywhere but under the card, so nothing of it reaches the card's own
/// ground.
struct PanelCardShadowMask: View {
  /// The card's outline.
  let shape: RoundedRectangle

  /// Everything round the card, with the card's outline cut away.
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
  #Preview("Mask") {
    PanelCardShadowMask(shape: RoundedRectangle(cornerRadius: PanelLayout.corner, style: .continuous))
      .frame(width: PanelLayout.width, height: PanelLayout.picture.height)
      .padding(PanelLayout.shadowRoom)
      .sampleSurface()
  }
#endif
