import AtlasService
import AtlasStatus
import AtlasUI
import Foundation

extension ViewerPresenter {
  /// Every fact about a session, as its row's tooltip and the Details part of its menu list them: its host, folder,
  /// start and when it was last seen or ended; the apps it opened as the Apps list last saw them, the copies Atlas made
  /// for it, the environment plans it applied with where each stands, and how many screenshots it took. The list's
  /// record says the most; a live session the list has not named yet has the status's own.
  ///
  /// - Parameters:
  ///   - session: The session as the status or the list has it; nil for one only the list names.
  ///   - record: The session as the list has it; nil when the list does not name it.
  ///   - snapshot: What the controller holds.
  /// - Returns: The facts, in order.
  static func sessionFacts(
    _ session: ViewerStatus.HostSession?,
    record: HostSessionRecord?,
    _ snapshot: ViewerSnapshot
  ) -> [Fact] {
    if let record {
      return sessionFacts(record, snapshot) + sessionApps(record, snapshot) + sessionCopies(record)
        + sessionPlans(record, snapshot) + sessionShots(record)
    }
    guard let session else { return [] }
    var facts = [hostFact(session.host, label: session.hostLabel, name: session.name)]
    if let folder = session.folder { facts.append(Fact(name: ViewerWords.folderFact, value: folder)) }
    if let started = session.startedAt {
      facts.append(Fact(name: ViewerWords.startedFact, value: minute(started, snapshot)))
    }
    if let seen = session.lastSeenAt {
      facts.append(Fact(name: ViewerWords.lastSeenFact, value: Words.ago(snapshot.date.timeIntervalSince(seen))))
    }
    return facts
  }

  /// A session's own facts: its host, or None named for Terminal; its folder when known; when it started; when it
  /// ended, or was last seen; and, while the sessions' list cannot be read, that what it says may be out of date.
  ///
  /// - Parameters:
  ///   - record: The session, as the list has it.
  ///   - snapshot: What the controller holds.
  /// - Returns: The facts.
  static func sessionFacts(_ record: HostSessionRecord, _ snapshot: ViewerSnapshot) -> [Fact] {
    var facts = [hostFact(record.host, label: record.hostLabel, name: record.name)]
    if let folder = record.folder { facts.append(Fact(name: ViewerWords.folderFact, value: folder)) }
    if let started = record.startedAt {
      facts.append(Fact(name: ViewerWords.startedFact, value: minute(started, snapshot)))
    }
    if let ended = record.endedAt {
      facts.append(Fact(name: ViewerWords.endedFact, value: Words.ago(snapshot.date.timeIntervalSince(ended))))
    } else if let seen = snapshot.reading.statusReading.session(record.key)?.lastSeenAt ?? record.lastSeenAt {
      facts.append(Fact(name: ViewerWords.lastSeenFact, value: Words.ago(snapshot.date.timeIntervalSince(seen))))
    }
    // The list kept from before may be out of date while it cannot be read.
    if snapshot.panes.sessions.failure != nil {
      facts.append(Fact(name: ViewerWords.sessionsFact, value: ViewerWords.sessionsUnread))
    }
    return facts
  }

  /// A session's host as its fact: the host's name, or None named for Terminal.
  ///
  /// - Parameters:
  ///   - host: The host.
  ///   - label: The host's own name for itself, if it gave one.
  ///   - name: The session's name.
  /// - Returns: The fact.
  private static func hostFact(_ host: SessionHost, label: String, name: String) -> Fact {
    Fact(
      name: ViewerWords.hostFact,
      value: host == .terminal ? ViewerWords.noHost : host.name(label: label, session: name)
    )
  }

  /// The apps a session opened: Open while the Apps list's last reading lists it, "Open · a copy" when the session
  /// opened a copy of it, Closed otherwise; before any reading, its name alone.
  ///
  /// - Parameters:
  ///   - record: The session, as the list has it.
  ///   - snapshot: What the controller holds.
  /// - Returns: The facts.
  static func sessionApps(_ record: HostSessionRecord, _ snapshot: ViewerSnapshot) -> [Fact] {
    let open = snapshot.panes.apps.apps.map { Set($0.map(\.name)) }
    return record.apps.map { app in
      guard let open else { return Fact(name: app.name, value: "") }
      let value =
        open.contains(app.name)
        ? (app.copied ? ViewerWords.appOpenCopy : ViewerWords.appOpen) : ViewerWords.appClosed
      return Fact(name: app.name, value: value)
    }
  }

  /// The copies Atlas made for a session, one fact an app: "Copy of Opie", or "Copies of Opie" with how many when it
  /// made more than one. The list keeps no time for a copy, so none is shown.
  ///
  /// - Parameter record: The session, as the list has it.
  /// - Returns: The facts.
  static func sessionCopies(_ record: HostSessionRecord) -> [Fact] {
    var apps: [String] = []
    var counts: [String: Int] = [:]
    for copy in record.copies where !copy.app.isEmpty {
      if counts[copy.app] == nil { apps.append(copy.app) }
      counts[copy.app, default: 0] += 1
    }
    return apps.map { app in
      let count = counts[app] ?? 1
      return Fact(name: ViewerWords.copyOf(app, count: count), value: count > 1 ? String(count) : "")
    }
  }

  /// The environment plans a session applied, one fact a plan: where it stands, and when it was made once known.
  ///
  /// - Parameters:
  ///   - record: The session, as the list has it.
  ///   - snapshot: What the controller holds.
  /// - Returns: The facts, such as "Environment plan" with "Applied · can roll back · made at 10:02".
  static func sessionPlans(_ record: HostSessionRecord, _ snapshot: ViewerSnapshot) -> [Fact] {
    record.plans.map { plan in
      let made = plan.madeAt.map { minute($0, snapshot) }
      return Fact(name: ViewerWords.planFact, value: ViewerWords.planLine(ViewerWords.planPhase(plan), madeAt: made))
    }
  }

  /// How many screenshots a session took, while it took any; Show Screenshots in its menu shows them.
  ///
  /// - Parameter record: The session, as the list has it.
  /// - Returns: The fact, or none.
  static func sessionShots(_ record: HostSessionRecord) -> [Fact] {
    guard !record.screenshots.isEmpty else { return [] }
    return [Fact(name: ViewerWords.shotsFact, value: String(record.screenshots.count))]
  }

  /// A moment to the minute, such as 09:12, the hour in two digits, on the person's own 12- or 24-hour clock.
  ///
  /// - Parameters:
  ///   - date: The moment.
  ///   - snapshot: What the controller holds.
  /// - Returns: "09:12".
  static func minute(_ date: Date, _ snapshot: ViewerSnapshot) -> String {
    let style = Date.FormatStyle(locale: snapshot.locale, timeZone: snapshot.timeZone)
      .hour(.twoDigits(amPM: .abbreviated))
      .minute(.twoDigits)
    return date.formatted(style)
  }
}
