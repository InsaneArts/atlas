import SwiftUI

extension Color {
  /// A colour that follows the appearance it is drawn in: `light` under Light and `dark` under Dark, so every
  /// window, the panel and the alerts change with System Settings › Appearance at once, with nothing reopened. A
  /// SwiftUI view picks its side by its colour scheme, and AppKit by the appearance it draws in, so the same token
  /// serves a window's background too. High contrast picks its nearer side. AppKit picks the side on the main thread,
  /// where every view draws, so a test that resolves one runs on the main actor.
  ///
  /// - Parameters:
  ///   - light: The colour under the light appearance.
  ///   - dark: The colour under the dark appearance.
  init(light: ColorValue, dark: ColorValue) {
    let light = light.nsColor
    let dark = dark.nsColor
    self.init(
      nsColor: NSColor(name: nil) { appearance in
        appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua ? dark : light
      }
    )
  }
}
