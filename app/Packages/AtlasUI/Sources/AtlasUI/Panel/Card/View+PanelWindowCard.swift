import SwiftUI

extension View {
  /// Draws the panel as its window holds it: hanging from the top of its card, which fills whatever height the window
  /// gives it. So a window easing to a new height carries the card's corners and shadow with it, while the panel's
  /// rows are already in their new places, cut by the card's edge. Round the card is the room ``PanelWindowFrame``
  /// names.
  ///
  /// - Parameter fits: Receives the height the panel takes by itself, each time it changes, so the window follows it.
  /// - Returns: The panel on its card, in its window's room.
  public func panelWindowCard(fits: @escaping @MainActor (CGFloat) -> Void) -> some View {
    fixedSize(horizontal: false, vertical: true)
      .onGeometryChange(for: CGFloat.self) { proxy in
        proxy.size.height
      } action: { height in
        fits(height)
      }
      .frame(minHeight: .zero, maxHeight: .infinity, alignment: .top)
      .background(PanelLayout.ground)
      .panelCard()
      .padding(.top, PanelWindowFrame.top)
      .padding([.horizontal, .bottom], PanelWindowFrame.side)
  }
}

#if DEBUG
  #Preview("Samples") {
    ModifierSamples(.panelWindowCard)
  }
#endif
