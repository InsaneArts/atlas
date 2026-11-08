import AsyncSupportTesting
import AtlasService
import AtlasServiceTesting
import AtlasUI
import Testing

@testable import AtlasApp

@Suite("Viewer Activity history")
struct ViewerActivityHistoryTests {
  @Test("a run the person opened stays open with every retained step as the oldest row leaves the log")
  func choicesSurviveHistoryAdvancing() async throws {
    let service = ScriptedService()
    service.activityLog = Self.log(0..<200)
    let viewer = try await ViewerController.watching(service)
    viewer.send(.show(.activity))
    try await settle("Activity was never read") { viewer.panes.activity.log != nil }
    let first = try #require(viewer.panes.activity.moments.first?.id)
    #expect(ViewerActivityFixtures.runs(ViewerPresenter.activity(viewer.snapshot)).first?.isOpen == false)
    viewer.send(.toggleRun(first, opens: true))
    service.activityLog = Self.log(1..<201)
    await viewer.panesModel.readActivity(viewer.panesModel.work.generation)
    #expect(viewer.panes.activity.moments.first?.id == first)
    #expect(viewer.panes.activity.choices[first] == true)
    let shown = ViewerPresenter.activity(viewer.snapshot)
    #expect(ViewerActivityFixtures.runs(shown).first?.id == first)
    #expect(ViewerActivityFixtures.runs(shown).first?.isOpen == true)
    #expect(ViewerActivityFixtures.steps(shown).count == 200, "the opened run shows every retained step")
    viewer.send(.toggleRun(first, opens: false))
    #expect(ViewerActivityFixtures.steps(ViewerPresenter.activity(viewer.snapshot)).isEmpty)
    viewer.close()
  }

  /// A full reading of distinct actions in one continuous run, newest first.
  ///
  /// - Parameter range: The retained row numbers.
  /// - Returns: The log.
  private static func log(_ range: Range<Int>) -> ActivityLog {
    ActivityLog(
      entries: range.reversed()
        .map { index in
          ActivityFixtures.row(
            id: "row-\(index)",
            says: "clicked Item \(index)",
            ageSeconds: Double(range.upperBound - index),
            verb: "click",
            app: "Notes"
          )
        }
    )
  }
}
