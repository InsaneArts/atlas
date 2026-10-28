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

    @Test("Sessions take Recent's place only while they have rows")
    func sessionsTakeRecentsPlace() throws {
      let height = { (state: PanelState) -> CGFloat in
        NSHostingView(rootView: PanelView(state: state) { _ in })
          .fittingSize
          .height
      }
      var recentOnly = PreviewStates.panelSessions
      recentOnly.sessions = []
      recentOnly.recent = PreviewStates.panelWorking.recent
      var both = PreviewStates.panelSessions
      both.recent = PreviewStates.panelWorking.recent
      #expect(height(both) == height(PreviewStates.panelSessions), "Recent draws nothing while Sessions show")
      #expect(height(recentOnly) == height(PreviewStates.panelWorking), "with no rows, Recent shows as before")
      #expect(height(both) != height(recentOnly))
    }
  }
#endif
