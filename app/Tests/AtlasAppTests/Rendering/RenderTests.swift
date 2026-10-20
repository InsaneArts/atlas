#if DEBUG
  import AppKit
  import Testing

  @testable import AtlasApp
  @testable import AtlasUI

  @MainActor
  @Suite("Render")
  struct RenderTests {
    /// A new empty folder for one test's renders, which the test removes.
    ///
    /// - Returns: The folder.
    /// - Throws: When the folder cannot be made.
    private static func directory() throws -> URL {
      let directory = FileManager.default.temporaryDirectory
        .appendingPathComponent("atlas-render-\(UUID().uuidString)")
      try FileManager.default
        .createDirectory(at: directory, withIntermediateDirectories: true)
      return directory
    }

    @Test("every onboarding render draws at its window's size")
    func everyOnboardingPreviewDraws() throws {
      let directory = try Self.directory()
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

    @Test("every viewer render draws at the samples' window size, wide enough to show the sidebar samples' sidebar")
    func everyViewerRenderDraws() throws {
      let size = PreviewStates.viewerSize
      try #require(
        ViewerLayout.showsSidebar(width: size.width, wanted: true),
        "the samples' window is wide enough for the sidebar"
      )
      let directory = try Self.directory()
      defer {
        try? FileManager.default
          .removeItem(at: directory)
      }
      #expect(ViewerRender.draw(to: directory.path))
      let drawn = try FileManager.default
        .contentsOfDirectory(atPath: directory.path)
      #expect(Set(drawn) == Set(ViewerRender.renders.map { "\($0.file).png" }))
      for render in ViewerRender.renders {
        let file = directory.appendingPathComponent("\(render.file).png")
        let image = try #require(NSBitmapImageRep(data: Data(contentsOf: file)), "\(render.file) was not drawn")
        #expect(image.pixelsWide == Int(size.width) * PNG.scale, "\(render.file)")
        #expect(image.pixelsHigh == Int(size.height) * PNG.scale, "\(render.file)")
      }
    }

    @Test("the viewer renders draw every sample the previews list")
    func theViewerRendersDrawEverySample() {
      let missing = PreviewStates.viewer
        .filter { sample in !ViewerRender.renders.contains { $0.state == sample.value } }
        .map(\.name)
      #expect(missing.isEmpty, "samples with no render: \(missing)")
    }

    @Test("a sidebar sample in a window too narrow for the sidebar draws without it")
    func aSidebarSampleDrawsInANarrowWindow() throws {
      let size = NSSize(width: 900, height: 600)
      try #require(!ViewerLayout.showsSidebar(width: size.width, wanted: true), "the window is too narrow for it")
      let directory = try Self.directory()
      defer {
        try? FileManager.default
          .removeItem(at: directory)
      }
      let file = directory.appendingPathComponent("narrow-sidebar.png")
      #expect(PNG.write(ViewerView(state: PreviewStates.viewerSidebar) { _ in }, size: size, to: file))
      let image = try #require(NSBitmapImageRep(data: Data(contentsOf: file)), "the narrow window was not drawn")
      #expect(image.pixelsWide == Int(size.width) * PNG.scale)
    }
  }
#endif
