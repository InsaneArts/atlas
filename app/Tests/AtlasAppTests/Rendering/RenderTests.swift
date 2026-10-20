#if DEBUG
  import AppKit
  import Testing

  @testable import AtlasApp
  import AtlasUI

  @MainActor
  @Suite("Render")
  struct RenderTests {
    @Test("every onboarding render draws at its window's size")
    func everyOnboardingPreviewDraws() throws {
      let directory = FileManager.default.temporaryDirectory
        .appendingPathComponent("atlas-render-\(UUID().uuidString)")
      try FileManager.default
        .createDirectory(at: directory, withIntermediateDirectories: true)
      defer {
        try? FileManager.default
          .removeItem(at: directory)
      }
      #expect(OnboardingRender.draw(to: directory.path))
      for render in OnboardingRender.renders {
        let file = directory.appendingPathComponent("\(render.file).png")
        let image = try #require(NSBitmapImageRep(data: Data(contentsOf: file)), "\(render.file) was not drawn")
        let size = OnboardingRender.size(render.state)
        #expect(image.pixelsWide == Int(size.width) * PNG.scale, "\(render.file)")
        #expect(image.pixelsHigh == Int(size.height) * PNG.scale, "\(render.file)")
      }
    }

    @Test("every viewer preview the design renders name draws to a PNG")
    func everyViewerRenderDraws() throws {
      let directory = FileManager.default.temporaryDirectory
        .appendingPathComponent("atlas-render-\(UUID().uuidString)")
      try FileManager.default
        .createDirectory(at: directory, withIntermediateDirectories: true)
      defer {
        try? FileManager.default
          .removeItem(at: directory)
      }
      #expect(ViewerRender.draw(to: directory.path, size: NSSize(width: 900, height: 600)))
      let drawn = try FileManager.default
        .contentsOfDirectory(atPath: directory.path)
      #expect(Set(drawn) == Set(ViewerRender.renders.map { "\($0.file).png" }))
    }
  }
#endif
