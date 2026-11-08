/// A task's replay over the whole window: its steps' marked pictures in order, then the screen after the last, one at a
/// time, played one a second or stepped through.
public struct ViewerReplayState: Equatable, Sendable {
  /// The task's title: the task the agent named, such as "Post the 0.1.1 release note"; else the session's name, such
  /// as "Claude Code · atlas"; else its app and when it started, such as "Slack · 10:31".
  public var title: String
  /// Who did it when the title does not say, how many steps and when, such as "Claude Code · atlas · 5 steps · 10:30:41
  /// to 10:31:10".
  public var detail: String
  /// The icon of the agent that did it; nil when unknown.
  public var icon: RowIcon?
  /// The frames, oldest step first, the screen after last.
  public var frames: [ViewerReplayFrame]
  /// The frame shown.
  public var index: Int
  /// Whether it plays.
  public var isPlaying: Bool

  /// Creates a replay.
  ///
  /// - Parameters:
  ///   - title: The task's title.
  ///   - detail: Who did it, how many steps and when.
  ///   - icon: The agent's icon; none by default.
  ///   - frames: The frames.
  ///   - index: The frame shown; the first by default.
  ///   - isPlaying: Whether it plays; it does not by default.
  public init(
    title: String,
    detail: String,
    icon: RowIcon? = nil,
    frames: [ViewerReplayFrame],
    index: Int = 0,
    isPlaying: Bool = false
  ) {
    self.title = title
    self.detail = detail
    self.icon = icon
    self.frames = frames
    self.index = index
    self.isPlaying = isPlaying
  }

  /// The frame shown, when there is one.
  public var frame: ViewerReplayFrame? {
    frames.indices.contains(index) ? frames[index] : nil
  }

  /// The frames grouped by the app they were in, in order, for the strip's labels; none when every frame was in one
  /// app, or an app is unknown.
  public var appGroups: [ViewerReplayAppGroup] {
    guard frames.allSatisfy({ !$0.app.isEmpty }), Set(frames.map(\.app)).count > 1 else { return [] }
    var groups: [ViewerReplayAppGroup] = []
    for (index, frame) in frames.enumerated() {
      if let last = groups.last, last.app == frame.app {
        groups[groups.count - 1].frames = last.frames.lowerBound..<index + 1
      } else {
        groups.append(ViewerReplayAppGroup(app: frame.app, frames: index..<index + 1))
      }
    }
    return groups
  }

  /// How many of its frames are steps, the screen after left out where there is one.
  public var steps: Int {
    frames.filter { !$0.isAfter }
      .count
  }

  /// Where it stands, such as "Step 1 of 5", or "After" on the screen after.
  public var position: String {
    guard let frame, !frame.isAfter else { return frame == nil ? "" : "After" }
    let step = frames[..<index].filter { !$0.isAfter }.count
    return "Step \(step + 1) of \(steps)"
  }
}
