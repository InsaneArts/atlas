#if DEBUG
  import AppKit
  import AtlasUI
  import SwiftUI
  import Testing

  @testable import AtlasApp

  // One render at a time: run together, the renders' seconds of main-actor drawing queue ahead of every other suite's
  // work and outlast its waits; one by one, that work runs between them.
  @Suite("Render", .serialized)
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

    // One case a render, so drawing them, about a second each with their frosted chips and alerts, never holds
    // the main actor long enough to starve another suite's wait.
    @Test("every main window render draws at the boards' window size", arguments: await ViewerRender.names)
    func everyViewerRenderDraws(file: String) throws {
      let size = PreviewStates.viewerSize
      let render = try #require(ViewerRender.renders.first { $0.name == file })
      #expect(render.size == size)
      let directory = try Self.directory()
      defer {
        try? FileManager.default
          .removeItem(at: directory)
      }
      #expect(ViewerRender.draw(file, render.value, to: directory, size: try #require(render.size)))
      let drawn = directory.appendingPathComponent("\(file).png")
      let image = try #require(NSBitmapImageRep(data: Data(contentsOf: drawn)), "\(file) was not drawn")
      #expect(image.pixelsWide == Int(size.width) * PNG.scale)
      #expect(image.pixelsHigh == Int(size.height) * PNG.scale)
    }

    @Test("every narrow main window render draws at its width", arguments: await ViewerRender.narrowNames)
    func everyNarrowRenderDraws(file: String) throws {
      let render = try #require(ViewerRender.narrowRenders.first { $0.name == file })
      let directory = try Self.directory()
      defer {
        try? FileManager.default
          .removeItem(at: directory)
      }
      let size = try #require(render.size)
      #expect(size.height == PreviewStates.viewerSize.height)
      #expect(ViewerRender.draw(file, render.value, to: directory, size: size))
      let drawn = directory.appendingPathComponent("\(file).png")
      let image = try #require(NSBitmapImageRep(data: Data(contentsOf: drawn)), "\(file) was not drawn")
      #expect(image.pixelsWide == Int(size.width) * PNG.scale)
    }

    @Test("the main window renders have names of their own")
    func theViewerRendersHaveTheirOwnNames() {
      let names = (ViewerRender.renders + ViewerRender.narrowRenders + ViewerRender.frameRenders)
        .map(\.name)
      #expect(Set(names).count == names.count)
    }

    @Test("every settings render draws at Settings' width and its pane's height", arguments: SettingsRender.files)
    func everySettingsRenderDraws(file: String) throws {
      let state = try #require(SettingsRender.renders.first { $0.file == file }?.state)
      let size = SettingsRender.size(of: state)
      #expect(size.width == SettingsLayout.width)
      #expect(size.height > SettingsLayout.header, "the pane said how tall it draws")
      #expect(size.height != SettingsLayout.firstHeight, "the pane said how tall it draws")
      let directory = try Self.directory()
      defer {
        try? FileManager.default
          .removeItem(at: directory)
      }
      #expect(SettingsRender.draw(file, state, to: directory))
      let drawn = directory.appendingPathComponent("\(file).png")
      let image = try #require(NSBitmapImageRep(data: Data(contentsOf: drawn)), "\(file) was not drawn")
      #expect(image.pixelsWide == Int(size.width) * PNG.scale)
      #expect(image.pixelsHigh == Int(size.height) * PNG.scale)
    }

    @Test("the settings renders draw every sample the previews list, each under a name of its own")
    func theSettingsRendersDrawEverySample() {
      #expect(SettingsRender.renders.count == PreviewStates.settings.count, "every sample has a name")
      #expect(Set(SettingsRender.files).count == SettingsRender.files.count)
    }

    @Test("every panel render draws the panel at its width on its card", arguments: PanelRender.modes)
    func everyPanelRenderDraws(mode: String) throws {
      let directory = try Self.directory()
      defer {
        try? FileManager.default
          .removeItem(at: directory)
      }
      let snapshot = PanelRenderOverrides(environment: ["ATLAS_RENDER_MODE": mode, "ATLAS_RENDER_PREVIEW": "1"])
        .applied(to: PanelSnapshot(picture: SampleDesktop.picture))
      let drawn = directory.appendingPathComponent("\(mode).png")
      #expect(PanelRender.draw(snapshot, to: drawn.path, appearance: Appearance.dark))
      let image = try #require(NSBitmapImageRep(data: Data(contentsOf: drawn)), "\(mode) was not drawn")
      // The panel's own width, as its view lays out alone, with the render's margin on both sides.
      let panel = NSHostingView(rootView: PanelView(state: PanelPresenter.state(snapshot)) { _ in })
        .fittingSize
        .width
      #expect(image.pixelsWide == Int(panel + 2 * PanelRender.margin) * PNG.scale)
    }

    /// The alert a render's settings draw, as the alert window's controller would hold it.
    ///
    /// - Parameter file: The render's file name.
    /// - Returns: The alert.
    /// - Throws: When the file names no alert render.
    private static func alert(_ file: String) throws -> AlertSnapshot {
      let environment = try #require(PanelRender.alerts.first { $0.file == file }?.environment)
      let overrides = PanelRenderOverrides(environment: environment)
      return try #require(overrides.alertSnapshot(over: overrides.applied(to: PanelSnapshot()).status.overview))
    }

    @Test("every alert window render draws its alert", arguments: PanelRender.alerts.map(\.file))
    func everyAlertRenderDraws(file: String) throws {
      let directory = try Self.directory()
      defer {
        try? FileManager.default
          .removeItem(at: directory)
      }
      let drawn = directory.appendingPathComponent("\(file).png")
      #expect(PanelRender.draw(alert: try Self.alert(file), to: drawn.path, appearance: Appearance.dark))
      let image = try #require(NSBitmapImageRep(data: Data(contentsOf: drawn)), "\(file) was not drawn")
      #expect(image.pixelsWide > 0 && image.pixelsHigh > 0)
    }

    @Test("every helper notice render draws the system alert", arguments: PanelRender.notices.map(\.file))
    func everyNoticeRenderDraws(file: String) throws {
      let directory = try Self.directory()
      defer {
        try? FileManager.default
          .removeItem(at: directory)
      }
      let environment = try #require(PanelRender.notices.first { $0.file == file }?.environment)
      let notice = try #require(PanelRenderOverrides(environment: environment).notice)
      let drawn = directory.appendingPathComponent("\(file).png")
      #expect(notice.draw(to: drawn.path, appearance: Appearance.dark))
      let image = try #require(NSBitmapImageRep(data: Data(contentsOf: drawn)), "\(file) was not drawn")
      #expect(image.pixelsWide > 0 && image.pixelsHigh > 0)
    }

    @Test("the alert window renders draw every alert sample the previews list, each under a name of its own")
    func theAlertRendersDrawEverySample() throws {
      let drawn = try PanelRender.alerts
        .map { AlertPresenter.state(try Self.alert($0.file)) }
      let missing = PreviewStates.alert
        .filter { !drawn.contains($0.value) }
        .map(\.name)
      #expect(missing.isEmpty, "samples with no render: \(missing)")
      #expect(Set(PanelRender.alerts.map(\.file)).count == PanelRender.alerts.count)
    }

    @Test("every component's samples draw, none of them empty")
    func everyComponentRenderDraws() throws {
      let directory = try Self.directory()
      defer {
        try? FileManager.default
          .removeItem(at: directory)
      }
      #expect(ComponentRender.draw(to: directory.path))
      for sample in ComponentSample.allCases {
        let file = directory.appendingPathComponent("\(sample.rawValue).png")
        let image = try #require(NSBitmapImageRep(data: Data(contentsOf: file)), "\(sample.rawValue) was not drawn")
        #expect(image.pixelsWide > 0 && image.pixelsHigh > 0, "\(sample.rawValue)")
      }
    }

    @Test("the viewer renders draw every sample the previews list")
    func theViewerRendersDrawEverySample() {
      let missing = PreviewStates.viewer
        .filter { sample in
          !ViewerRender.renders
            .contains { $0.value == sample.value }
        }
        .map(\.name)
      #expect(missing.isEmpty, "samples with no render: \(missing)")
    }
  }
#endif
