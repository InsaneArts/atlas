import AtlasKit
import Foundation

/// The Screenshots tab's words that depend on what it holds: the count, the day groups, a screenshot's label, caption
/// and file name, and the empty messages. Fixed copy, such as the pane's title and its search field's prompt, stays in
/// its view.
extension ViewerWords {
  /// The screenshots could not be read: the headline.
  static let shotsFailed = "Atlas couldn't read the screenshots"
  /// No screenshot was taken: the headline.
  static let noShots = "No screenshots yet"
  /// No screenshot was taken: the sentence.
  static let noShotsMessage = "Screenshots agents take in the helper show up here."
  /// A search that matches nothing: the sentence.
  static let noShotsMatchMessage = "Search by the time one was taken, such as 10:31."

  /// How many screenshots were taken today, and how to use them, beside the title.
  ///
  /// - Parameter today: The count.
  /// - Returns: "6 today · Space opens one, drag one out to use it".
  static func shotsMeta(today: Int) -> String {
    "\(today == 0 ? "None" : String(today)) today · Space opens one, drag one out to use it"
  }

  /// A group of screenshots' label.
  ///
  /// - Parameters:
  ///   - period: The stretch of time.
  ///   - day: A day's name, for a day before yesterday.
  /// - Returns: "Today", "This morning", "This afternoon", "This evening", "Yesterday", the day, or "Earlier".
  static func shotPeriod(_ period: ShotPeriod, day: String) -> String {
    switch period {
    case .lastHour: "Today"
    case .morning: "This morning"
    case .afternoon: "This afternoon"
    case .evening: "This evening"
    case .yesterday: "Yesterday"
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

  /// A search by time that matches nothing: the headline.
  ///
  /// - Parameter query: The search.
  /// - Returns: "No screenshots at “9:15”".
  static func noShotsMatch(_ query: String) -> String {
    "No screenshots at “\(query.trimmingCharacters(in: .whitespacesAndNewlines))”"
  }
}
