import AtlasUI
import CoreGraphics

/// The window's shape (v3 G2): each pane says how tall it draws, and the window fits the one shown, its title naming
/// it. Only a height leaves SwiftUI; the window resizes in AppKit.
extension SettingsController {
  /// A pane drew at a height: kept for whenever it shows, and the window fits it now if it shows.
  ///
  /// - Parameters:
  ///   - tab: The pane.
  ///   - height: Its height, its insets included and the tab row not.
  func fit(_ tab: SettingsTab, height: CGFloat) {
    guard paneHeights[tab] != height else { return }
    paneHeights[tab] = height
    if tab == session.tab { reshapeWindow() }
  }

  /// The window's content height: the title and tabs over the pane shown, or nil until that pane has drawn.
  var windowHeight: CGFloat? {
    paneHeights[session.tab]
      .map { SettingsLayout.header + $0 }
  }

  /// Hands the window the pane shown: its name, and the height to fit when known.
  func reshapeWindow() {
    reshape?(SettingsWords.tab(session.tab), windowHeight)
  }
}
