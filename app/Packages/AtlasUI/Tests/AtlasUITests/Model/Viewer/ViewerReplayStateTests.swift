import Testing

@testable import AtlasUI

@Suite("ViewerReplayState")
struct ViewerReplayStateTests {
  /// A replay whose frames were in `apps`, in order, the last the screen after where `after` says.
  ///
  /// - Parameters:
  ///   - apps: Each frame's app.
  ///   - after: Whether the last frame is the screen after; it is not by default.
  /// - Returns: The replay.
  static func replay(_ apps: [String], after: Bool = false) -> ViewerReplayState {
    ViewerReplayState(
      title: "Post the 0.1.1 release note",
      detail: "",
      frames: apps.enumerated()
        .map { index, app in
          ViewerReplayFrame(
            id: "\(index)",
            picture: ViewerActivityShot(name: "\(index)-before.jpg"),
            words: ViewerStepWords("Clicked"),
            detail: "",
            label: "\(index + 1)",
            app: app,
            isAfter: after && index == apps.count - 1
          )
        }
    )
  }

  @Test("frames in more than one app group by app in order, each run its own group")
  func groupsByApp() {
    let groups = Self.replay(["Safari", "Slack", "Slack", "Safari"]).appGroups
    #expect(groups.map(\.app) == ["Safari", "Slack", "Safari"])
    #expect(groups.map(\.frames) == [0..<1, 1..<3, 3..<4])
  }

  @Test("frames all in one app, or with an app unknown, are not grouped")
  func noGroupsForOneApp() {
    #expect(Self.replay(["Slack", "Slack", "Slack"]).appGroups.isEmpty)
    #expect(Self.replay(["Safari", "", "Slack"]).appGroups.isEmpty)
    #expect(Self.replay([]).appGroups.isEmpty)
  }

  @Test("a replay with the screen after counts its steps without it and stands on After there")
  func positionWithAfter() {
    var replay = Self.replay(["Slack", "Slack", "Slack"], after: true)
    #expect(replay.steps == 2)
    #expect(replay.position == "Step 1 of 2")
    replay.index = 2
    #expect(replay.position == "After")
  }

  @Test("a replay with no screen after counts every frame as a step, its last one included")
  func positionWithoutAfter() {
    var replay = Self.replay(["Slack", "Slack", "Slack"])
    #expect(replay.steps == 3)
    replay.index = 2
    #expect(replay.position == "Step 3 of 3")
  }
}
