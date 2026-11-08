/// What a session mostly did and where, as its title says it: the kind of action it did most, keys counted as typing,
/// how many of that kind, and the app it acted in most. Every word of it is something Atlas saw happen.
public struct ActivityGist: Equatable, Sendable {
  /// The kind of action done most: typing for keys pressed too, never control changing hands.
  public var kind: ActivityAction
  /// How many lines did that kind.
  public var count: Int
  /// The app acted in most, by name; nil when no line named one.
  public var app: String?

  /// Creates a gist.
  ///
  /// - Parameters:
  ///   - kind: The kind of action done most.
  ///   - count: How many lines did it.
  ///   - app: The app acted in most.
  public init(kind: ActivityAction, count: Int, app: String?) {
    self.kind = kind
    self.count = count
    self.app = app
  }

  /// The gist of `kinds` done in `app`: the kind done most, keys pressed counted as typing; among equals, the one
  /// first in ``preference``. None while nothing but control changed hands.
  ///
  /// - Parameters:
  ///   - kinds: How many lines did each kind.
  ///   - app: The app acted in most.
  init?(kinds: [ActivityAction: Int], app: String?) {
    var counted: [ActivityAction: Int] = [:]
    for (kind, count) in kinds where kind != .control {
      let said = kind == .keys ? ActivityAction.typing : kind
      counted[said] = Self.adding(count, to: counted[said] ?? 0)
    }
    guard
      let most = Self.preference.max(by: { (counted[$0] ?? 0, -Self.rank($0)) < (counted[$1] ?? 0, -Self.rank($1)) }),
      let count = counted[most], count > 0
    else { return nil }
    self.init(kind: most, count: count, app: app)
  }

  /// The gist a session's record keeps once its rows are gone: how many of each verb it did, and how many of its
  /// actions named each app. A verb this app does not know is left out.
  ///
  /// - Parameters:
  ///   - counts: How many of each verb, such as `type` or `click`.
  ///   - apps: How many actions named each app, by its name.
  public init?(counts: [String: Int], apps: [String: Int]) {
    var kinds: [ActivityAction: Int] = [:]
    for (verb, count) in counts {
      guard let kind = ActivityAction.verb(verb) else { continue }
      kinds[kind] = Self.adding(count, to: kinds[kind] ?? 0)
    }
    let app = apps.filter { $0.value > 0 }
      .max { ($0.value, $1.key) < ($1.value, $0.key) }?
      .key
    self.init(kinds: kinds, app: app)
  }

  /// The gist of a session's rows: each row's kind, and the app most rows acted in. The person's own rows are not a
  /// session's, and a row refused before anything was sent did nothing.
  ///
  /// - Parameter entries: The session's rows.
  public init?(_ entries: [ActivityLog.Entry]) {
    var kinds: [ActivityAction: Int] = [:]
    var apps: [String: Int] = [:]
    for entry in entries where entry.actor != .you && entry.outcome != .notSent {
      let kind = ActivityAction(entry)
      kinds[kind] = Self.adding(1, to: kinds[kind] ?? 0)
      if let app = entry.app, !app.isEmpty { apps[app] = Self.adding(1, to: apps[app] ?? 0) }
    }
    let app = apps.max { ($0.value, $1.key) < ($1.value, $0.key) }?
      .key
    self.init(kinds: kinds, app: app)
  }

  /// The kinds a gist can say, the first winning among equals.
  static let preference: [ActivityAction] = [.typing, .pointer, .window, .scroll, .screenshot, .command]

  /// A count added to a total, saturating, so counts a record kept as large as they come never trap.
  ///
  /// - Parameters:
  ///   - count: The count.
  ///   - total: The total so far.
  /// - Returns: The total, capped at the largest count.
  private static func adding(_ count: Int, to total: Int) -> Int {
    let (sum, overflow) = total.addingReportingOverflow(count)
    return overflow ? .max : sum
  }

  /// A kind's place in ``preference``.
  ///
  /// - Parameter kind: The kind.
  /// - Returns: Its place; every kind a gist can say is listed.
  private static func rank(_ kind: ActivityAction) -> Int {
    preference.firstIndex(of: kind) ?? preference.count
  }
}
