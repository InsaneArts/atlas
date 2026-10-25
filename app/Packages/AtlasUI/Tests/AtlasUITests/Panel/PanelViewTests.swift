#if DEBUG
  import SwiftUI
  import Testing

  @testable import AtlasUI

  @MainActor
  @Suite("PanelView")
  struct PanelViewTests {
    @Test("every panel state draws the panel at its own width, whatever it holds")
    func everyStateDrawsAtThePanelWidth() throws {
      for preview in PreviewStates.panel {
        let renderer = ImageRenderer(content: PanelView(state: preview.value) { _ in })
        renderer.scale = 1
        let image = try #require(renderer.cgImage, "\(preview.name) was not drawn")
        #expect(image.width == Int(PanelLayout.width), "\(preview.name)")
      }
    }
  }
#endif
