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
    message: "Agents work freely and nothing asks you. If a task needs an account, sign the helper in here."
  )

  /// Why a change to the list can't be asked yet: the menu bar's connection to the service isn't up.
  static let noService = "Atlas isn't connected to its service yet. Try again in a moment."

  /// The app panel's button.
  static let addAccountPrompt = "Add"
  /// The app panel's line.
  static let addAccountMessage = "Choose an app the helper is signed in to."

  /// When an account was added: "added today", "added yesterday" or "added Nov 5".
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
      style.locale = calendar.locale ?? .autoupdatingCurrent
      day = added.formatted(style)
    }
    return "added \(day)"
  }

  /// An account's line in the list: its level, then when it was added.
  ///
  /// - Parameters:
  ///   - account: The account.
  ///   - now: Now.
  ///   - calendar: The calendar days are told in.
  /// - Returns: "Ask before sending · added today", or "Trusted · App · added Nov 5".
  static func accountRowDetail(_ account: ViewerStatus.Account, now: Date, calendar: Calendar) -> String {
    let level = levelShort(account.level)
    let added = accountDetail(account, now: now, calendar: calendar)
    return account.kind == .app ? "\(level) · App · \(added)" : "\(level) · \(added)"
  }

  /// What an account is and when it was added, under its name on its page.
  ///
  /// - Parameters:
  ///   - account: The account.
  ///   - now: Now.
  ///   - calendar: The calendar days are told in.
  /// - Returns: "Website · added today", or "App · added Nov 5".
  static func accountKindLine(_ account: ViewerStatus.Account, now: Date, calendar: Calendar) -> String {
    let kind = account.kind == .site ? "Website" : "App"
    return "\(kind) · \(accountDetail(account, now: now, calendar: calendar))"
  }

  /// A level in a sentence's case, as an account's line names it.
  ///
  /// - Parameter level: The level.
  /// - Returns: "Look only", "Ask before sending" or "Trusted".
  static func levelShort(_ level: AccountLevel) -> String {
    switch level {
    case .lookOnly: "Look only"
    case .ask: "Ask before sending"
    case .trusted: "Trusted"
    }
  }

  /// The note Atlas shows once after it added an account, with what its level lets agents do there.
  ///
  /// - Parameters:
  ///   - name: The account's name.
  ///   - level: Its level; Ask Before Sending, a new account's, by default.
  /// - Returns: "Added github.com after you typed a password there. Agents ask you before sending there."
  static func accountAdded(_ name: String, level: AccountLevel = .ask) -> String {
    "Added \(name) after you typed a password there. \(levelLine(level))"
  }

  /// What an account's level lets agents do, as the note says it.
  ///
  /// - Parameter level: The level.
  /// - Returns: "Agents ask you before sending there.", say.
  static func levelLine(_ level: AccountLevel) -> String {
    SignInWords.level(level)
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

  /// Under the list while General keeps no pictures of agent actions: Activity says what agents did in an account, but
  /// keeps no picture of it.
  static let accountsPicturesOff = "Activity won't keep pictures of what agents do here."

  /// A change to the list that failed.
  ///
  /// - Parameter message: What the service said.
  /// - Returns: The line.
  static func accountsFailed(_ message: String) -> String {
    "Atlas couldn't change its accounts list. \(Words.sentence(message))"
  }
}
