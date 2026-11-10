import AtlasUI
import Foundation

/// The Activity pane's words that depend on what it holds: the filters, the runs, a step's words, what followed a
/// problem, and the empty messages. Fixed copy stays in its view.
extension ViewerWords {
  /// The Activity filters, for VoiceOver.
  static let activityFiltersLabel = "Which actions"
  /// The heading over yesterday's sessions.
  static let yesterdayGroup = "Yesterday"
  /// A run of commands, and anything else done in no app.
  static let commandsMoment = "Commands"
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
  /// A session whose steps were all cleared from the log: the headline.
  static let stepsCleared = "Steps were cleared"
  /// A session whose steps were all cleared from the log: the sentence.
  static let stepsClearedMessage = "This session's steps were cleared to save space."
  /// A search that matches nothing: the sentence.
  static let noActivityMatchMessage = "Search looks at what was done and where, such as “clicked” or “Opie”."
  /// Problems, with none: the headline.
  static let noProblems = "No problems"
  /// Problems, with none: the sentence.
  static let noProblemsMessage = "Every action here worked."
  /// Pictures or Typing, with nothing: the headline.
  static let noneFiltered = "No actions here"
  /// Pictures or Typing, with nothing: the sentence.
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
  /// - Returns: "All", "Problems", "Pictures" or "Typing".
  static func filter(_ filter: ViewerActivityFilter) -> String {
    switch filter {
    case .all: "All"
    case .problems: "Problems"
    case .pictures: "Pictures"
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

  /// What a step did before a reason: the next step's words after "Then it" or "Then you".
  ///
  /// - Parameters:
  ///   - words: The next step's words, with any actor prefix already removed.
  ///   - personal: Whether the person performed the action.
  /// - Returns: "Then it typed into Name.", or "Then you clicked." for the person's own.
  static func then(_ words: String, personal: Bool) -> String {
    let subject = personal ? "you" : "it"
    return "Then \(subject) \(words.prefix(1).lowercased() + words.dropFirst())."
  }

  /// A search that matches nothing: the headline.
  ///
  /// - Parameter query: The search.
  /// - Returns: "No actions match “Safari”".
  static func noActivityMatch(_ query: String) -> String {
    "No actions match “\(query.trimmingCharacters(in: .whitespacesAndNewlines))”"
  }
}
