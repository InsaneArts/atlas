import AtlasService
import AtlasUI
import Foundation

/// The Accounts pane's words: what it says with none, when each account was added, the note that Atlas added one, and
/// the question before one is taken off the list. Atlas lists accounts; it doesn't prove a sign-in, so nothing here
/// says the helper is signed in.
extension SettingsWords {
  /// What the pane says with no accounts listed, with Add an App….
  static let accountsNone = EmptyMessageState(
    headline: "No accounts yet",
    message: "Agents work freely and nothing asks you. If a task needs an account, sign the helper in here.",
    action: EmptyMessageAction(title: "Add an App…", isPrimary: false)
  )

  /// Why a change to the list can't be asked yet: the menu bar's connection to the service isn't up.
  static let noService = "Atlas isn't connected to its service yet. Try again in a moment."

  /// The app panel's button.
  static let addAccountPrompt = "Add"
  /// The app panel's line.
  static let addAccountMessage = "Choose an app the helper is signed in to."

  /// When an account was added: "Added today", "Added yesterday" or "Added Nov 5", after "App · " for an app.
  ///
  /// - Parameters:
  ///   - account: The account.
  ///   - now: Now.
  ///   - calendar: The calendar days are told in.
  /// - Returns: The line.
  static func accountDetail(_ account: ViewerStatus.Account, now: Date, calendar: Calendar) -> String {
    let added = Date(timeIntervalSince1970: account.added / 1000)
    let day: String
    if calendar.isDate(added, inSameDayAs: now) {
      day = "today"
    } else if let yesterday = calendar.date(byAdding: .day, value: -1, to: now),
      calendar.isDate(added, inSameDayAs: yesterday)
    {
      day = "yesterday"
    } else {
      var style = Date.FormatStyle(date: .omitted, time: .omitted)
        .month(.abbreviated)
        .day()
      style.calendar = calendar
      style.timeZone = calendar.timeZone
      day = added.formatted(style)
    }
    return account.kind == .app ? "App · added \(day)" : "Added \(day)"
  }

  /// The note Atlas shows once after it added an account.
  ///
  /// - Parameter name: The account's name.
  /// - Returns: "Added github.com after you typed a password there."
  static func accountAdded(_ name: String) -> String {
    "Added \(name) after you typed a password there."
  }

  /// The question before an account is taken off the list.
  ///
  /// - Parameter name: The account's name.
  /// - Returns: The alert's title.
  static func removeAccountTitle(_ name: String) -> String {
    "Remove \(name) from Accounts?"
  }

  /// What taking an account off the list does, and doesn't.
  static let removeAccountMessage = "Atlas stops listing it. Removing it from the list doesn't sign the helper out."
  /// The alert's button that takes it off the list.
  static let removeAccount = "Remove"
  /// While it is taken off.
  static let removingAccount = "Removing…"

  /// A change to the list that failed.
  ///
  /// - Parameter message: What the service said.
  /// - Returns: The line.
  static func accountsFailed(_ message: String) -> String {
    "Atlas couldn't change its accounts list. \(Words.sentence(message))"
  }
}
