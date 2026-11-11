import SwiftUI

/// The room the menu bar panel's window keeps round its card (`panelWindowCard(fits:)`): the gap under the menu bar
/// above it, and its shadow's reach beside and below it. The window's shell reads these to size the window for a card
/// and to tell a click on the card from one beside it.
public enum PanelWindowFrame {
  /// 6: above the card, the gap between the menu bar and the card's top.
  public static let top = PanelLayout.barGap
  /// 68: beside the card and under it, as far as its shadow may spread.
  public static let side = PanelLayout.shadowRoom

  /// The window's size for a card of a height: the card's width and height in whole points, and the room round it.
  ///
  /// - Parameter height: The card's height.
  /// - Returns: The window's size.
  public static func size(card height: CGFloat) -> CGSize {
    CGSize(width: PanelLayout.width + side * 2, height: height.rounded(.up) + top + side)
  }

  /// Where the card is in a window of a size, measured from the window's bottom left, as AppKit measures.
  ///
  /// - Parameter window: The window's size.
  /// - Returns: The card's frame.
  public static func card(in window: CGSize) -> CGRect {
    CGRect(x: side, y: side, width: window.width - side * 2, height: window.height - side - top)
  }
}
