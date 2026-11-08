import AsyncSupportTesting
import SwiftUI
import Testing

@testable import AtlasApp
@testable import AtlasUI

@Suite("Your files motion", .serialized, AppKitState())
@MainActor
struct FilesSwapMotionTests {
  @Test("private and open swap in place: every sixtieth of a second the row keeps its height and the button its frame")
  func theSwapHoldsItsGeometry() async throws {
    let previous = Motion.Pose.moment
    Motion.Pose.hold(at: -5)
    defer { Motion.Pose.hold(at: previous) }
    let host = FilesSectionHost()
    let open = PreviewStates.settingsFilesOpen
    host.show(PreviewStates.settingsFilesPrivate)
    await Self.render(host)
    let row = try #require(host.frames["files home row"])
    let button = try #require(host.frames["files home button"])
    #expect(row.height > 0 && button.width > 0)
    Motion.Pose.hold(at: .zero)
    host.show(open)
    for frame in 0...30 {
      Motion.Pose.hold(at: Double(frame) / 60)
      await Self.render(host)
      #expect(host.frames["files home row"]?.height == row.height, "frame \(frame)")
      #expect(host.frames["files home button"] == button, "frame \(frame)")
    }
    host.show(PreviewStates.settingsFilesWorking)
    await Self.render(host)
    #expect(host.frames["files home button"] == button, "the spinner takes the title's place")
  }

  /// Draws a few frames, letting queued work run between them.
  ///
  /// - Parameter host: The section's host.
  private static func render(_ host: FilesSectionHost) async {
    for _ in 0..<6 {
      await afterQueuedWork()
      host.render()
    }
  }
}
