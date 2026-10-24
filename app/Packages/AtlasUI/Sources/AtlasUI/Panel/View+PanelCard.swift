import SwiftUI

extension View {
  /// Draws the panel as the board does where no popover holds it, as in a design render or a preview: its corners,
  /// its fine dark ring and light inner edge, and its shadow.
  ///
  /// - Returns: The panel on its card.
  public func panelCard() -> some View {
    let shape = RoundedRectangle(cornerRadius: PanelLayout.corner)
    return
      clipShape(shape)
      .overlay(
        shape
          .strokeBorder(Color.strokeStrong, lineWidth: Stroke.hairline)
      )
      .background(
        shape
          .fill(PanelLayout.ground)
          .shadow(color: PanelLayout.shadowColor, radius: PanelLayout.shadowRadius, y: PanelLayout.shadowDrop)
      )
  }
}
