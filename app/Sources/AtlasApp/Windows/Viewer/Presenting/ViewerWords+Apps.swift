import Foundation

/// The Apps tab's words that depend on what it holds: unnamed apps and windows, the count, a window's labels and the
/// empty messages. Fixed copy, such as the pane's title and its search field's prompt, stays in its view.
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
  /// A search that matches nothing: the sentence.
  static let noAppsMatchMessage = "Search looks at the names of apps and their windows."

  /// How many apps and windows are open, beside the title.
  ///
  /// - Parameters:
  ///   - apps: The apps.
  ///   - windows: Their windows.
  /// - Returns: "5 apps · 6 windows", or nothing with no app open, where the pane says why.
  static func appsMeta(apps: Int, windows: Int) -> String {
    guard apps > 0 else { return "" }
    let appWord = apps == 1 ? "1 app" : "\(apps) apps"
    switch windows {
    case 0: return "\(appWord) · no windows"
    case 1: return "\(appWord) · 1 window"
    default: return "\(appWord) · \(windows) windows"
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
}
