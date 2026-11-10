import SwiftUI
import Testing

@testable import AtlasUI

@Suite("Viewer motion poses")
struct ViewerMotionPoseTests {
  @MainActor
  @Test("every list pose holds each row once, before its change and after")
  func everyListPoseHoldsEachRowOnce() {
    let states = PreviewStates.viewerMotion
      .flatMap { [($0.name, $0.from), ($0.name, $0.to)] }
    for (name, state) in states {
      switch state.pane {
      case .activity(let activity):
        let ids = activity.days.flatMap(Self.rows(of:))
        #expect(Set(ids).count == ids.count, "\(name) draws a row twice")
      case .screen:
        break
      }
    }
  }

  private static func rows(of day: ViewerActivityDay) -> [String] {
    let title: [String] = day.title.isEmpty ? [] : ["day.\(day.id)"]
    let more: [String] = day.more.map { ["more.\($0.day)"] } ?? []
    return title + day.sessions.flatMap(rows(in:)) + more
  }

  private static func rows(in session: ViewerOutlineSession) -> [String] {
    let runs = session.runs.flatMap { run in ["run.\(run.id)"] + run.steps.map { "step.\($0.id)" } }
    return ["session.\(session.id)"] + runs
  }

  @MainActor
  @Test("every pose changes what the window shows, and each list pose stays on its list")
  func everyPoseChangesSomething() {
    for pose in PreviewStates.viewerMotion {
      #expect(pose.from != pose.to, "\(pose.name) changes nothing")
      if case .list = pose.lead {
        #expect(pose.from.pane.place == pose.to.pane.place, "\(pose.name) leaves its list")
      }
    }
  }

  @MainActor
  @Test("a list change's key frames stand in the middle of each beat, in order, and the last once all has settled")
  func aListChangesFramesStandInEachBeat() throws {
    let pose = try #require(PreviewStates.viewerMotion.first { $0.name == "run-arrive" })
    let moments = ViewerMotionPose.quarters
      .map(pose.moment(at:))
    #expect(moments == moments.sorted())
    #expect(Set(moments).count == moments.count)
    let rate = Motion.playbackRate
    // The rows leaving half faded, then a row arriving half faded in once the room is made.
    #expect(abs(moments[1] - 0.05 / rate) < 1e-9)
    #expect(abs(moments[3] - 0.375 / rate) < 1e-9)
    // The last frame stands where the move has settled and the last fade has ended.
    let settled = (0.1 + Motion.Springs.state.settlingDuration) / rate
    #expect(moments[4] >= settled - 1e-9)
    #expect(moments[4] >= 0.45 / rate)
  }
}
