import Foundation

/// Where the menu bar panel's window goes: hanging from the menu bar's lower edge, its card centred under Atlas's
/// item, and moved in from a side of the screen the card would otherwise cross.
enum PanelPlacement {
  /// 8: the least room between the card and a side of the screen.
  static let margin: CGFloat = 8

  /// The window's frame.
  ///
  /// - Parameters:
  ///   - size: The window's size, the room round its card included.
  ///   - anchor: The menu bar item's frame on its screen.
  ///   - visible: The screen's frame clear of the menu bar and the Dock.
  ///   - room: The clear room the window keeps at each side of its card.
  /// - Returns: The frame, on whole points across.
  static func frame(size: CGSize, under anchor: CGRect, in visible: CGRect, room: CGFloat) -> CGRect {
    let centred = anchor.midX - size.width / 2
    let least = visible.minX + margin - room
    let most = max(least, visible.maxX - margin + room - size.width)
    let across = min(max(centred, least), most)
    return CGRect(x: across.rounded(), y: anchor.minY - size.height, width: size.width, height: size.height)
  }
}
