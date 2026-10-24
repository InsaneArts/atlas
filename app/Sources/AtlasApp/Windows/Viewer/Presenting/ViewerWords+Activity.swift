import AtlasUI
import Foundation

/// The Activity tab's words that depend on what it holds: the filters, the groups, the count, a line's words and the
/// empty messages. Fixed copy, such as the pane's title and its search field's prompt, stays in its view.
extension ViewerWords {
  /// The Activity filters, for VoiceOver.
  static let activityFiltersLabel = "Which actions"
  /// The group of what was done in the last two minutes.
  static let nowGroup = "Now"
  /// The group of what was done earlier today.
  static let earlierTodayGroup = "Earlier today"
  /// The group of what was done yesterday.
  static let yesterdayGroup = "Yesterday"
  /// The log could not be read: the headline.
  static let activityFailed = "Atlas couldn't read the activity"
  /// Nothing has been done in the helper: the headline.
  static let noActivity = "Nothing has happened yet"
  /// Nothing has been done in the helper: the sentence.
  static let noActivityMessage = "What agents and you do in the helper shows up here."
  /// A search that matches nothing: the sentence.
  static let noActivityMatchMessage = "Search looks at what was done, such as “clicked” or “Opie”."
  /// Problems, with none: the headline.
  static let noProblems = "No problems"
  /// Problems, with none: the sentence.
  static let noProblemsMessage = "Every action here worked."
  /// Agent or You, with nothing: the headline.
  static let noneFiltered = "No actions here"
  /// Agent or You, with nothing: the sentence.
  static let noneFilteredMessage = "Choose All to see every action."

  /// A filter's word.
  ///
  /// - Parameter filter: The filter.
  /// - Returns: "All", "Agent", "You" or "Problems".
  static func filter(_ filter: ViewerActivityFilter) -> String {
    switch filter {
    case .all: "All"
    case .agent: "Agent"
    case .you: "You"
    case .problems: "Problems"
    }
  }

  /// How many actions were done today, beside the title.
  ///
  /// - Parameter count: The count.
  /// - Returns: "38 actions today", "1 action today" or "No actions today".
  static func actionsToday(_ count: Int) -> String {
    switch count {
    case 0: "No actions today"
    case 1: "1 action today"
    default: "\(count) actions today"
    }
  }

  /// A line's words with how many times it was done in a row.
  ///
  /// - Parameters:
  ///   - text: What was done.
  ///   - count: How many times in a row.
  /// - Returns: "You clicked in Opie ×3", or the words alone for once.
  static func repeated(_ text: String, count: Int) -> String {
    count > 1 ? "\(text) ×\(count)" : text
  }

  /// What VoiceOver reads for a line.
  ///
  /// - Parameters:
  ///   - time: When it started.
  ///   - text: What was done.
  ///   - reason: Why it failed, if it did.
  ///   - live: Whether it runs now.
  /// - Returns: "10:28:30, Couldn't click Save, No element is named Save", or with ", now" while it runs.
  static func lineLabel(time: String, text: String, reason: String?, live: Bool) -> String {
    ([time, text] + [reason, live ? "now" : nil].compactMap { $0 })
      .joined(separator: ", ")
  }

  /// A search that matches nothing: the headline.
  ///
  /// - Parameter query: The search.
  /// - Returns: "No actions match “Safari”".
  static func noActivityMatch(_ query: String) -> String {
    "No actions match “\(query.trimmingCharacters(in: .whitespacesAndNewlines))”"
  }
}
