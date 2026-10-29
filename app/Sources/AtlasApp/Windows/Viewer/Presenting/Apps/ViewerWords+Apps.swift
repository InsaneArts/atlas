import Foundation

/// The Apps tab's words that depend on what it holds: unnamed apps and windows, the counts, the groups, the cards'
/// lines and tags, a window's labels and the empty messages. Fixed copy stays in its view.
extension ViewerWords {
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
  /// The chosen session opened no app: the headline.
  static let noSessionApps = "This session hasn't opened any apps"
  /// The chosen session opened no app: the sentence.
  static let noSessionAppsMessage = "Apps it opens in the helper show up here. All sessions shows every app."
  /// A search that matches nothing: the sentence.
  static let noAppsMatchMessage = "Search looks at the names of apps and their windows."
  /// The group of apps agents opened.
  static let openedByAgents = "Opened by agents"
  /// The group of apps open before any agent opened them, and the line of one whose launch is not known.
  static let alreadyOpen = "Already open"
  /// The line of an app a session opened at a time not known, while only that session's apps show.
  static let openedSometime = "Opened"
  /// When an app was opened under a minute ago.
  static let openedNow = "just now"
  /// The tag of the app in front.
  static let inFront = "In front"
  /// The tag of a hidden app.
  static let hidden = "Hidden"
  /// The tag of an app run from a copy Atlas made.
  static let fromCopy = "From a copy"
  /// The tag of an app one of a session's tasks installed.
  static let installedByAtlas = "Installed by Atlas"

  /// How many apps are open, as the summary's title says it.
  ///
  /// - Parameter count: The apps.
  /// - Returns: "1 app open" or "5 apps open".
  static func appsOpen(_ count: Int) -> String {
    count == 1 ? "1 app open" : "\(count) apps open"
  }

  /// How many windows the apps have, beside the summary's title.
  ///
  /// - Parameter count: The windows.
  /// - Returns: "no windows", "1 window" or "8 windows".
  static func windowCount(_ count: Int) -> String {
    switch count {
    case 0: "no windows"
    case 1: "1 window"
    default: "\(count) windows"
    }
  }

  /// When an app was opened: in minutes within the hour, then on the clock, then by its day before today.
  ///
  /// - Parameters:
  ///   - seconds: How long ago.
  ///   - clock: The time of day it was opened, on the person's clock.
  ///   - day: The day it was opened, before today; nil for today.
  /// - Returns: "just now", "4 min ago", "at 09:12" or "on Mon 19 Oct".
  static func openedWhen(seconds: Double, clock: String, day: String?) -> String {
    if let day { return "on \(day)" }
    if seconds < 60 { return openedNow }
    if seconds < 3600 { return "\(Int((seconds / 60).rounded(.down))) min ago" }
    return "at \(clock)"
  }

  /// An app's line while only the session that opened it shows.
  ///
  /// - Parameter when: When it was opened (``openedWhen(seconds:clock:day:)``).
  /// - Returns: "Opened 4 min ago".
  static func opened(_ when: String) -> String {
    "Opened \(when)"
  }

  /// The line of an app already open.
  ///
  /// - Parameter since: The time it launched, today, or its day before.
  /// - Returns: "Open since 09:12".
  static func openSince(_ since: String) -> String {
    "Open since \(since)"
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
}
