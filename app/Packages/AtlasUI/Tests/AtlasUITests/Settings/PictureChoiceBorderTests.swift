import SwiftUI
import Testing

@testable import AtlasUI

@Suite("PictureChoiceBorder")
struct PictureChoiceBorderTests {
  @Test("keyboard focus adds an outer halo to both selected and unselected cadence cards")
  @MainActor
  func focusAddsAHaloRegardlessOfSelection() throws {
    for selected in [false, true] {
      let pictures = try [false, true].map { focused in
        try PanelPixels(
          PictureChoiceBorder(selected: selected, focused: focused)
            .frame(width: SettingsLayout.pictureChoiceWidth, height: SettingsLayout.pictureChoiceHeight)
            .padding(SettingsLayout.focusHalo)
        )
      }
      let x = Int(SettingsLayout.focusHalo) - 1
      let y = pictures[0].height / 2
      #expect(pictures[0].colour(x: x, y: y).alpha == 0, "selection alone has no outer halo")
      #expect(pictures[1].colour(x: x, y: y).alpha > 0, "focus remains visible when selected: \(selected)")
      let edge = pictures[0].colour(x: Int(SettingsLayout.focusHalo), y: y)
      #expect((edge.alpha > 0) == selected, "selection keeps its own inner edge")
    }
  }
}
