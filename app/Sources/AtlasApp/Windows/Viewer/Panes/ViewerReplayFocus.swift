/// The task replayed over the window: which run, the frame shown, and whether it plays.
struct ViewerReplayFocus: Equatable {
  /// The run replayed.
  var moment: String
  /// The frame shown.
  var index = 0
  /// Whether it plays, one frame a second.
  var isPlaying = false
}
