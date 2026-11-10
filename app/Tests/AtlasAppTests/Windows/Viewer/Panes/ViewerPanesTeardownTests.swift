import AsyncSupportTesting
import AtlasServiceTesting
import Foundation
import Testing

@testable import AtlasApp

@Suite("Viewer panes teardown")
struct ViewerPanesTeardownTests {
  @Test("closing the window cancels the picture sheet's read before a late picture arrives")
  func closingCancelsTheSheetsRead() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerPanesModelTests.onShots(service)
    defer {
      viewer.close()
      service.resumeAllShots()
    }
    let name = ViewerPanesModelTests.newer
    viewer.send(.openShot(name))
    let pictures = viewer.panesModel.pictures
    let load = try #require(pictures.loads[name])
    try await settle("the picture was never requested") {
      service.heldShots.contains { $0.name == name && $0.full }
    }
    viewer.close()
    #expect(load.isCancelled)
    #expect(pictures.loads.isEmpty)
    service.resumeAllShots()
    try await load.value("the cancelled picture never replied")
    #expect(viewer.panes.activity.pictures[name] == nil)
  }

  @Test("closing the window cancels Copy and Save before either delivers", arguments: [false, true])
  func closingCancelsPictureExports(saves: Bool) async throws {
    let service = ScriptedService()
    var deliveries: [Data] = []
    let viewer = try await ViewerPanesModelTests.onShots(
      service,
      shotOutlet: ShotOutlet(
        copy: { png, _ in
          deliveries.append(png)
          return true
        },
        reveal: { _ in true },
        save: { data, _ in
          deliveries.append(data)
          return true
        }
      )
    )
    defer {
      viewer.close()
      service.resumeAllShots()
    }
    viewer.send(.openShot(ViewerPanesModelTests.newer))
    if saves { viewer.panesModel.pictures.save {} } else { viewer.send(.copyPicture) }
    let pictures = viewer.panesModel.pictures
    let pending = try #require(pictures.copying)
    // The sheet reads the picture to show it, and the export reads it again.
    try await settle("the export was never requested") { service.heldShots.count == 2 }
    viewer.close()
    #expect(pending.isCancelled)
    #expect(pictures.copying == nil)
    service.resumeAllShots()
    try await pending.value("the cancelled export never replied")
    #expect(deliveries.isEmpty)
  }

  @Test("closing the window cancels thumbnails and a late reply starts no queued work")
  func closingCancelsThumbnails() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerPanesModelTests.onShots(service)
    defer {
      viewer.close()
      service.resumeAllShots()
    }
    for name in ["first.png", "second.png", "queued.png"] { viewer.send(.thumbnailNeeded(name)) }
    try await settle("the thumbnail reads never started") { service.heldShots.count == 2 }
    let thumbnails = viewer.panesModel.thumbnails
    let pending = Array(thumbnails.work.loading.values)
    viewer.close()
    #expect(pending.count == 2)
    for task in pending { #expect(task.isCancelled) }
    #expect(thumbnails.work.loading.isEmpty)
    service.resumeAllShots()
    for task in pending { try await task.value("a cancelled thumbnail never replied") }
    #expect(service.shotCalls == 2)
    for name in ["first.png", "second.png", "queued.png"] { #expect(viewer.panes.thumbnails[name] == nil) }
  }
}
