import AtlasUI
import Foundation

/// The Activity tab's words that depend on what it holds: the filters, the summary, the moments, a step's words and
/// times, what followed a problem, and the empty messages. Fixed copy, such as Show large, stays in its view.
extension ViewerWords {
  /// The Activity filters, for VoiceOver.
  static let activityFiltersLabel = "Which actions"
  /// The group of what was done in the last two minutes, as Screenshots heads it.
  static let nowGroup = "Now"
  /// The group of what was done earlier today, as Screenshots heads it.
  static let earlierTodayGroup = "Earlier today"
  /// The heading over yesterday's moments, and Screenshots' group of yesterday's.
  static let yesterdayGroup = "Yesterday"
  /// A moment of commands, and anything else done in no app.
  static let commandsMoment = "Commands"
  /// A moment of screenshots alone.
  static let screenshotsMoment = "Screenshots"
  /// A command that ran.
  static let ranCommand = "Ran a command"
  /// A command that runs now.
  static let runningCommand = "Running a command"
  /// A command that failed; its reason says why.
  static let commandFailed = "A command failed"
  /// The log could not be read: the headline.
  static let activityFailed = "Atlas couldn't read the activity"
  /// Nothing has been done in the helper: the headline.
  static let noActivity = "Nothing has happened yet"
  /// Nothing has been done in the helper: the sentence.
  static let noActivityMessage = "What agents and you do in the helper shows up here."
  /// A search that matches nothing: the sentence.
  static let noActivityMatchMessage = "Search looks at what was done and where, such as “clicked” or “Opie”."
  /// Problems, with none: the headline.
  static let noProblems = "No problems"
  /// Problems, with none: the sentence.
  static let noProblemsMessage = "Every action here worked."
  /// Screenshots or Typing, with nothing: the headline.
  static let noneFiltered = "No actions here"
  /// Screenshots or Typing, with nothing: the sentence.
  static let noneFilteredMessage = "Choose All to see every action."
  /// A problem's retry that worked.
  static let workedOnRetry = "The agent tried again, and it worked."
  /// The person's own retry that worked.
  static let youWorkedOnRetry = "You tried again, and it worked."
  /// A problem's retry that failed too.
  static let failedAgain = "It failed again."

  /// A filter's word.
  ///
  /// - Parameter filter: The filter.
  /// - Returns: "All", "Problems", "Screenshots" or "Typing".
  static func filter(_ filter: ViewerActivityFilter) -> String {
    switch filter {
    case .all: "All"
    case .problems: "Problems"
    case .screenshots: "Screenshots"
    case .typing: "Typing"
    }
  }

  /// How many actions were done today, for VoiceOver.
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

  /// A count's word, in the count's own number.
  ///
  /// - Parameters:
  ///   - count: The count.
  ///   - one: The word for one, such as "action".
  /// - Returns: "action" for one, else the word with an s, such as "actions".
  static func counted(_ count: Int, _ one: String) -> String {
    count == 1 ? one : "\(one)s"
  }

  /// How much a folded moment held and when.
  ///
  /// - Parameters:
  ///   - actions: How many actions it held.
  ///   - start: When it started, on the person's clock.
  ///   - end: When it ended.
  /// - Returns: "9 actions · 10:21–10:24", or "1 action · 10:21" when it began and ended in one minute.
  static func momentSpan(actions: Int, start: String, end: String) -> String {
    let when = start == end ? start : "\(start)–\(end)"
    return "\(actions) \(counted(actions, "action")) · \(when)"
  }

  /// What a step did before a reason: the next step's words after "Then it" or "Then you".
  ///
  /// - Parameter words: The next step's words, such as "Typed into Name" or "You clicked".
  /// - Returns: "Then it typed into Name.", or "Then you clicked." for the person's own.
  static func then(_ words: String) -> String {
    if words.hasPrefix("You ") { return "Then you \(words.dropFirst(4))." }
    return "Then it \(words.prefix(1).lowercased() + words.dropFirst())."
  }

  /// When a step was done, or how long ago a folded moment ended: "now" under 10 seconds, then seconds and minutes
  /// within the hour, else the time on the person's clock.
  ///
  /// - Parameters:
  ///   - seconds: How long ago.
  ///   - clock: The time on the person's clock, for longer ago.
  /// - Returns: "now", "4 s", "3 min" or "10:31".
  static func stepAge(_ seconds: TimeInterval, clock: String) -> String {
    if seconds < 10 { return "now" }
    if seconds < 60 { return "\(Int(seconds.rounded(.down))) s" }
    if seconds < 3600 { return "\(Int((seconds / 60).rounded(.down))) min" }
    return clock
  }

  /// A search that matches nothing: the headline.
  ///
  /// - Parameter query: The search.
  /// - Returns: "No actions match “Safari”".
  static func noActivityMatch(_ query: String) -> String {
    "No actions match “\(query.trimmingCharacters(in: .whitespacesAndNewlines))”"
  }
}
