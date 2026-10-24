import AppKit
import Testing

@testable import AtlasUI

/// Here, not in AtlasUI's tests, because measuring a font's line height needs AppKit, which AtlasUI never imports.
@Suite("TextStyle line height")
struct TextStyleLineHeightTests {
  @Test("each style's line spacing makes its lines v3's line height, measured against the system font")
  func lineSpacingMakesTheLineHeight() {
    let layout = NSLayoutManager()
    for style in TextStyle.allCases {
      guard let lineHeight = style.lineHeight else { continue }
      let weight: NSFont.Weight = style.defaultWeight == .semibold ? .semibold : .regular
      let font = NSFont.systemFont(ofSize: style.pointSize, weight: weight)
      let drawn = layout.defaultLineHeight(for: font) + style.lineSpacing
      #expect(abs(drawn - lineHeight) <= 0.5, "\(style): \(drawn) for \(lineHeight)")
    }
  }
}
