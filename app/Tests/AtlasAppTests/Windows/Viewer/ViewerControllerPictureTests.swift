import AtlasServiceTesting
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@MainActor
@Suite("ViewerController+Picture")
struct ViewerControllerPictureTests {
  @Test("Copy Picture never asks the helper for a new frame")
  func copyPictureUsesTheFrameOnScreen() async throws {
    let service = ScriptedService()
    let pictures = RecordedPictures()
    let viewer = try await ViewerController.watching(
      service,
      nap: ViewerController.untilCancelled,
      pictures: pictures.outlet
    )
    let asked = service.frameCalls
    let shown = try #require(viewer.reading.picture, "a picture shows")
    viewer.copyPicture()
    #expect(service.frameCalls == asked)
    #expect(pictures.copied.count == 1)
    #expect(pictures.copied.first === shown.image)
    viewer.close()
  }
}
