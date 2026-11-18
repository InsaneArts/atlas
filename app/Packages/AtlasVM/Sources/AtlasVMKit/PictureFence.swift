/// What tells a station whether a VM's window was opened between two of its
/// readings: a count of this run's own, advanced each time the window is
/// opened with input, before the window can take any. A station takes a
/// picture only between two readings that show the same run, the same count
/// and the window closed.
struct PictureFence: Equatable, Sendable {
  /// How many times this run began opening the window with input.
  private(set) var value: UInt64 = 0
  /// How many openings are under way now.
  private var opening = 0
  /// How many openings ended, done or not.
  private var opened: UInt64 = 0

  /// A fence no opening has advanced yet.
  init() {}

  /// The window is about to be opened with input: the count goes up first.
  mutating func beginOpening() {
    value += 1
    opening += 1
  }

  /// An opening begun ended, whether or not the window opened.
  mutating func endOpening() {
    opening -= 1
    opened += 1
  }

  /// Whether the window may have been opened since `reading` was taken of
  /// this fence: an opening is under way now, or one ended since. A window
  /// seen closed after that is not to be said closed for that reading.
  func mayHaveOpened(since reading: PictureFence) -> Bool {
    opening > 0 || opened != reading.opened
  }
}
