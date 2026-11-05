/// A run of a replay's frames in one app, which the strip labels with the app's name over them.
public struct ViewerReplayAppGroup: Equatable, Sendable, Identifiable {
  /// The app's name, such as "Slack".
  public var app: String
  /// The frames it holds, by their places in the replay.
  public var frames: Range<Int>

  /// Creates a group.
  ///
  /// - Parameters:
  ///   - app: The app's name.
  ///   - frames: The frames it holds.
  public init(app: String, frames: Range<Int>) {
    self.app = app
    self.frames = frames
  }

  /// Each group starts at a frame of its own.
  public var id: Int { frames.lowerBound }
}
