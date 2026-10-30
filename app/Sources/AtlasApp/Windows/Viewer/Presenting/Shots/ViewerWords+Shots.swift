import AtlasService
import Foundation

/// The Screenshots pane's words that depend on what it holds: the count, the time groups, which are Activity's, a
/// screenshot's label, caption and file name, and the empty messages. Fixed copy, such as the pane's title and its
/// tiles' tooltip, stays in its view.
extension ViewerWords {
  /// The screenshots could not be read: the headline.
  static let shotsFailed = "Atlas couldn't read the screenshots"
  /// No screenshot was taken: the headline.
  static let noShots = "No screenshots yet"
  /// No screenshot was taken: the sentence.
  static let noShotsMessage = "Screenshots agents take in the helper show up here."

  /// How many screenshots were taken today, beside the title.
  ///
  /// - Parameter today: The count.
  /// - Returns: "6 today", or "None today".
  static func shotsMeta(today: Int) -> String {
    today == 0 ? "None today" : "\(today) today"
  }

  /// A group of screenshots' label, in the Activity pane's words: the last hour is Now, the rest of today one group.
  ///
  /// - Parameters:
  ///   - period: The stretch of time.
  ///   - day: A day's name, for a day before yesterday.
  /// - Returns: "Now", "Earlier today", "Yesterday", the day, or "Earlier".
  static func shotPeriod(_ period: ShotPeriod, day: String) -> String {
    switch period {
    case .lastHour: nowGroup
    case .morning, .afternoon, .evening: earlierTodayGroup
    case .yesterday: yesterdayGroup
    case .day: day
    case .undated: "Earlier"
    }
  }

  /// What VoiceOver reads for a screenshot's tile.
  ///
  /// - Parameter time: When it was taken.
  /// - Returns: "Screenshot taken at 10:31:44".
  static func shotLabel(_ time: String) -> String {
    "Screenshot taken at \(time)"
  }

  /// The large view's caption: when it was taken and its size.
  ///
  /// - Parameters:
  ///   - time: When it was taken.
  ///   - size: Its size in pixels, when known.
  /// - Returns: "10:31:44 · 2560 × 1440", or the time alone.
  static func shotCaption(time: String, size: String?) -> String {
    size.map { "\(time) · \($0)" } ?? time
  }

  /// What a screenshot dragged out is called, as a file: its time with dots, which a file name may hold, for colons.
  ///
  /// - Parameter time: When it was taken.
  /// - Returns: "Atlas screenshot 10.31.44".
  static func shotFileName(_ time: String) -> String {
    "Atlas screenshot \(time.replacingOccurrences(of: ":", with: "."))"
  }

}
