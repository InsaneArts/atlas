import AsyncSupportTesting
import AtlasService
import AtlasServiceTesting
import AtlasUI
import Testing

@testable import AtlasApp

@Suite("Viewer Activity history")
struct ViewerActivityHistoryTests {
  @Test("fold and expansion choices survive the oldest retained row leaving the log")
  func choicesSurviveHistoryAdvancing() async throws {
    let service = ScriptedService()
    service.activityLog = Self.log(0..<200)
    let viewer = try await ViewerController.watching(service)
    viewer.send(.show(.activity))
    try await settle("Activity was never read") { viewer.panes.activity.log != nil }
    let first = try #require(viewer.panes.activity.moments.first?.id)
    viewer.send(.toggleMoment(first, opens: false))
    viewer.send(.showMomentSteps(first))
    service.activityLog = Self.log(1..<201)
    await viewer.panesModel.readActivity(viewer.panesModel.work.generation)
    #expect(viewer.panes.activity.moments.first?.id == first)
    #expect(viewer.panes.activity.choices[first] == false)
    #expect(viewer.panes.activity.expanded.contains(first))
    let shown = ViewerPresenterActivityTests.moments(ViewerPresenter.activity(viewer.snapshot))
    #expect(shown.first?.id == first)
    #expect(shown.first?.isOpen == false)
    viewer.send(.toggleMoment(first, opens: true))
    let reopened = ViewerPresenter.activity(viewer.snapshot)
    #expect(ViewerPresenterActivityTests.steps(reopened).count == 200, "the expansion still shows every retained step")
    viewer.close()
  }

  /// A full reading of distinct actions in one continuous moment, newest first.
  ///
  /// - Parameter range: The retained row numbers.
  /// - Returns: The log.
  private static func log(_ range: Range<Int>) -> ActivityLog {
    ActivityLog(
      entries: range.reversed()
        .map { index in
          ActivityLog.Entry(
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
