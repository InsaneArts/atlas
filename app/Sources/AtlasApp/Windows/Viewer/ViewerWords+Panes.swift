import AtlasKit
import AtlasUI

/// The words of the Activity, Apps and Screenshots tabs that depend on what they hold: counts, group labels, a
/// line's words and the empty messages. Fixed copy, such as a pane's title and a search field's prompt, stays in its
/// view.
extension ViewerWords {
  // MARK: Activity

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
    ([time, text] + [reason, live ? "now" : nil].compactMap { $0 }).joined(separator: ", ")
  }

  /// A search that matches nothing: the headline.
  ///
  /// - Parameter query: The search.
  /// - Returns: "No actions match “Safari”".
  static func noActivityMatch(_ query: String) -> String {
    "No actions match “\(query.trimmingCharacters(in: .whitespacesAndNewlines))”"
  }

  // MARK: Apps

  /// A window with no title.
  static let untitledWindow = "Untitled window"
  /// An app the service gives no name for.
  static let unnamedApp = "Unnamed app"
  /// The apps could not be listed: the headline.
  static let appsFailed = "Atlas couldn't list the apps"
  /// No app is open: the headline.
  static let noApps = "No apps are open in the helper"
  /// No app is open: the sentence.
  static let noAppsMessage = "Apps an agent opens show up here."
  /// A search that matches nothing: the sentence.
  static let noAppsMatchMessage = "Search looks at the names of apps and their windows."

  /// How many apps and windows are open, beside the title.
  ///
  /// - Parameters:
  ///   - apps: The apps.
  ///   - windows: Their windows.
  /// - Returns: "5 apps · 6 windows, front to back", or "No apps".
  static func appsMeta(apps: Int, windows: Int) -> String {
    guard apps > 0 else { return "No apps" }
    let appWord = apps == 1 ? "1 app" : "\(apps) apps"
    switch windows {
    case 0: return "\(appWord) · no windows"
    case 1: return "\(appWord) · 1 window"
    default: return "\(appWord) · \(windows) windows, front to back"
    }
  }

  /// What VoiceOver calls Show on screen for a window.
  ///
  /// - Parameter title: The window's title.
  /// - Returns: "Show Ring settings on the screen".
  static func showWindowLabel(_ title: String) -> String {
    "Show \(title) on the screen"
  }

  /// An outlined window's label.
  ///
  /// - Parameters:
  ///   - app: The app's name.
  ///   - window: The window's title.
  /// - Returns: "Opie · Ring settings".
  static func outlineTitle(app: String, window: String) -> String {
    "\(app) · \(window)"
  }

  /// A search that matches nothing: the headline.
  ///
  /// - Parameter query: The search.
  /// - Returns: "No apps match “Xcode”".
  static func noAppsMatch(_ query: String) -> String {
    "No apps match “\(query.trimmingCharacters(in: .whitespacesAndNewlines))”"
  }

  // MARK: Screenshots

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
