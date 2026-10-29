import AppKit
import AtlasUI
import SwiftUI

extension RowIcon {
  /// Each icon's menu image once drawn, by what it shows and the scale it was drawn at. Only the main actor, which
  /// builds menus, reads and writes it. It needs no bound: the icons are those of the hosts ``SessionHost`` names, at
  /// the few scales a Mac's displays have.
  @MainActor private static var menuImages: [String: NSImage] = [:]

  /// The icon as a menu item's image, ``SessionIcon/menuSide`` points square, drawn once per icon and scale through
  /// ``SessionIcon`` and kept for the app's life.
  ///
  /// - Parameter scale: The display's backing scale to draw at.
  /// - Returns: The image, or nil should drawing fail.
  @MainActor func menuImage(scale: CGFloat) -> NSImage? {
    let key = "\(menuKey)@\(scale)"
    if let drawn = Self.menuImages[key] { return drawn }
    let side = SessionIcon.menuSide
    let renderer = ImageRenderer(content: SessionIcon(icon: self, side: side))
    renderer.scale = scale
    guard let image = renderer.cgImage else { return nil }
    let drawn = NSImage(cgImage: image, size: NSSize(width: side, height: side))
    Self.menuImages[key] = drawn
    return drawn
  }

  /// What the icon shows, as a key no other icon shares: an app's icon by its id, a mark by its kind, a tile by its
  /// symbol or characters.
  private var menuKey: String {
    switch self {
    case .app(let image): "app:\(image.id)"
    case .mark(let kind): "mark:\(kind)"
    case .tile(.symbol(let name)): "symbol:\(name)"
    case .tile(.text(let text)): "text:\(text)"
    }
  }
}
