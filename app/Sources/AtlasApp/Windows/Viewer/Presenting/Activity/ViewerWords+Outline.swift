import AtlasService
import Foundation

/// The Activity outline's words: what a run and a session did, a session's title from what it mostly did, the time
/// column's spans, the row for a long day's other sessions, and where a picture stands in the sheet.
extension ViewerWords {
  /// 5: the most sessions a day lists before the row for the rest.
  static let mostSessions = 5

  /// What a run did: its kinds of action in the order first done, then how many actions, such as "Typed, pressed keys
  /// and clicked · 1,212 actions"; for a run in no app, how many commands and screenshots, such as "Ran 3 commands,
  /// took 2 screenshots".
  ///
  /// - Parameters:
  ///   - tally: What the run held.
  ///   - app: The app it was in; nil for a run in no app.
  /// - Returns: The summary.
  static func runSummary(_ tally: ActivityTally, app: String?) -> String {
    let phrases = tally.order.compactMap {
      phrase($0, count: tally.kinds[$0] ?? 0, app: app, verbs: tally.verbs[$0] ?? [])
    }
    guard app != nil else {
      return Words.leadingCapital(phrases.joined(separator: ", "))
    }
    let done = Words.leadingCapital(listed(phrases))
    return dotted([done, actions(tally.actions)])
  }

  /// What a session did: the apps it worked in, how many actions and how many screenshots, such as "TextEdit and
  /// Chrome · 5,044 actions · 341 screenshots".
  ///
  /// - Parameter tally: What the session held.
  /// - Returns: The summary.
  static func sessionSummary(_ tally: ActivityTally) -> String {
    let apps =
      tally.apps.count > 3
      ? listed(Array(tally.apps.prefix(2)) + ["\(tally.apps.count - 2) more"]) : listed(tally.apps)
    let shots = tally.screenshots > 0 ? "\(number(tally.screenshots)) \(counted(tally.screenshots, "screenshot"))" : ""
    return dotted([apps, actions(tally.actions), shots])
  }

  /// How many problems, such as "1 problem"; empty for none.
  ///
  /// - Parameter count: How many.
  /// - Returns: The words.
  static func problems(_ count: Int) -> String {
    count > 0 ? "\(number(count)) \(counted(count, "problem"))" : ""
  }

  /// How many actions, such as "1,212 actions".
  ///
  /// - Parameter count: How many.
  /// - Returns: The words.
  static func actions(_ count: Int) -> String {
    "\(number(count)) \(counted(count, "action"))"
  }

  /// A session's title from what it mostly did and where: "Typing in TextEdit" while it works, "Typed in TextEdit"
  /// once it ends, "Opened TextEdit", or with no app "Took 2 screenshots" or "Ran a command".
  ///
  /// - Parameters:
  ///   - gist: What it mostly did.
  ///   - working: Whether it works now.
  /// - Returns: The title.
  static func sessionTitle(_ gist: ActivityGist, working: Bool) -> String {
    let count = gist.count
    let app = gist.app ?? ""
    let inApp = app.isEmpty ? "" : " in \(app)"
    switch gist.kind {
    case .typing, .keys: return (working ? "Typing" : "Typed") + inApp
    case .pointer: return Words.leadingCapital(pointer(gist.verbs, working: working)) + inApp
    case .scroll: return (working ? "Scrolling" : "Scrolled") + inApp
    case .window: return Words.leadingCapital(window(gist.verbs, working: working, object: app))
    case .screenshot:
      if working { return count == 1 ? "Taking a screenshot" : "Taking screenshots" }
      return count == 1 ? "Took a screenshot" : "Took \(number(count)) screenshots"
    case .command, .control:
      if working { return count == 1 ? "Running a command" : "Running commands" }
      return count == 1 ? "Ran a command" : "Ran \(number(count)) commands"
    }
  }

  /// The time column's span: "01:08–now" while under way, "01:49–01:50", or "01:50" within one minute.
  ///
  /// - Parameters:
  ///   - start: When it started.
  ///   - end: When it ended.
  ///   - running: Whether it is under way.
  /// - Returns: The span.
  static func span(_ start: String, _ end: String, running: Bool) -> String {
    if running { return "\(start)–now" }
    return start == end ? start : "\(start)–\(end)"
  }

  /// The row for the rest of a long day's sessions: "46 more sessions on Wednesday", or "3 more sessions today".
  ///
  /// - Parameters:
  ///   - count: How many more.
  ///   - day: The day's heading, or empty for today.
  /// - Returns: The row's words.
  static func moreSessions(_ count: Int, day: String) -> String {
    let more = "\(number(count)) more \(counted(count, "session"))"
    switch day {
    case "", todayGroup: return "\(more) today"
    case yesterdayGroup: return "\(more) yesterday"
    default: return "\(more) on \(day)"
    }
  }

  /// Where a picture stands among its run's, such as "120 of 120".
  ///
  /// - Parameters:
  ///   - index: Its place, from 0.
  ///   - count: How many.
  /// - Returns: The words.
  static func pictureOf(_ index: Int, _ count: Int) -> String {
    "\(number(index + 1)) of \(number(count))"
  }

  /// Which run a picture is of, such as "in this TextEdit run".
  ///
  /// - Parameter name: The run's name.
  /// - Returns: The words.
  static func inRun(_ name: String) -> String {
    "in this \(name) run"
  }

  /// What a picture saved from the sheet is called, as a file: its time with dots, which a file name may hold, for
  /// colons.
  ///
  /// - Parameter time: When it was taken.
  /// - Returns: "Atlas picture 10.31.44".
  static func pictureFileName(_ time: String) -> String {
    "Atlas picture \(time.replacingOccurrences(of: ":", with: "."))"
  }

  /// The heading over today, while the outline spans more than one day.
  static let todayGroup = "Today"
  /// The sheet's line beside a step's picture before it.
  static let justBefore = "just before this step"
  /// The sheet's line beside a step's picture after it.
  static let justAfter = "just after this step"
  /// The words a screenshot's step says.
  static let tookScreenshot = "Took a screenshot"
  /// What VoiceOver calls a picture in the outline.
  static let screenshotWord = "Screenshot"

  /// A kind of action as a run's summary says it, in the past, such as "pressed keys" or "took 2 screenshots".
  ///
  /// - Parameters:
  ///   - kind: The kind.
  ///   - count: How many lines did it.
  ///   - app: The run's app, which an app opened names.
  ///   - verbs: The verbs Atlas wrote for that kind's lines.
  /// - Returns: The phrase, or nil for none.
  private static func phrase(_ kind: ActivityAction, count: Int, app: String?, verbs: Set<String>) -> String? {
    switch kind {
    case .pointer: pointer(verbs, working: false)
    case .typing: "typed"
    case .keys: "pressed keys"
    case .scroll: "scrolled"
    case .window: window(verbs, working: false, object: app ?? "")
    case .control: "handed over control"
    case .screenshot: count == 1 ? "took a screenshot" : "took \(number(count)) screenshots"
    case .command: count == 1 ? "ran a command" : "ran \(number(count)) commands"
    }
  }

  /// What the pointer did, by the verbs Atlas wrote: "clicked" for clicks and presses, "dragged", "moved the pointer",
  /// or "used the pointer" for a mix or verbs not known; while working, "clicking" and so on.
  ///
  /// - Parameters:
  ///   - verbs: The pointer's verbs.
  ///   - working: Whether it is under way.
  /// - Returns: The phrase.
  static func pointer(_ verbs: Set<String>, working: Bool) -> String {
    if !verbs.isEmpty, verbs.isSubset(of: clicks) { return working ? "clicking" : "clicked" }
    if verbs == ["drag"] { return working ? "dragging" : "dragged" }
    if verbs == ["move"] { return working ? "moving the pointer" : "moved the pointer" }
    return working ? "using the pointer" : "used the pointer"
  }

  /// What was done to apps, from the verbs Atlas wrote, each said once in a fixed order: "opened TextEdit", "closed
  /// apps", "opened and installed TextEdit", "cleaned up", or "worked with apps" while the verbs are not known; while
  /// working, "opening TextEdit" and so on.
  ///
  /// - Parameters:
  ///   - verbs: The window's verbs.
  ///   - working: Whether it is under way.
  ///   - object: The app's name; empty says "apps".
  /// - Returns: The phrase.
  static func window(_ verbs: Set<String>, working: Bool, object: String) -> String {
    let apps = object.isEmpty ? "apps" : object
    let said = appVerbs.filter { verbs.contains($0.verb) }
      .map { working ? $0.working : $0.done }
      .reduce(into: [String]()) { words, word in
        if !words.contains(word) { words.append(word) }
      }
    let cleaned = verbs.contains("cleanup") ? [working ? "cleaning up" : "cleaned up"] : []
    guard !said.isEmpty else {
      return cleaned.first ?? (working ? "working with " : "worked with ") + apps
    }
    return listed(["\(listed(said)) \(apps)"] + cleaned)
  }

  /// The verbs done to apps, in the order a summary says them, with their words.
  static let appVerbs: [(verb: String, done: String, working: String)] = [
    ("open", "opened", "opening"), ("open-link", "opened", "opening"), ("close", "closed", "closing"),
    ("install", "installed", "installing"),
  ]

  /// The pointer's verbs that click or press a control.
  static let clicks: Set<String> = [
    "click", "double-click", "right-click", "press", "check", "uncheck", "pick", "expand", "collapse",
  ]

  /// Phrases as a list: "a", "a and b", "a, b and c".
  ///
  /// - Parameter phrases: The phrases.
  /// - Returns: The list.
  static func listed(_ phrases: [String]) -> String {
    guard let last = phrases.last else { return "" }
    guard phrases.count > 1 else { return last }
    let rest = phrases.dropLast()
      .joined(separator: ", ")
    return "\(rest) and \(last)"
  }

  /// A count with the locale's grouping, such as "5,044".
  ///
  /// - Parameter count: The count.
  /// - Returns: The number.
  static func number(_ count: Int) -> String {
    count.formatted(.number)
  }
}
