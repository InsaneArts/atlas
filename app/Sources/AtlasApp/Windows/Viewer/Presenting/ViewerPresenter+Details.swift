import AtlasService
import AtlasStatus
import AtlasUI
import Foundation

extension ViewerPresenter {
  /// Details: the chosen host session, when the sessions' list names it, then the helper, its readiness checks, the
  /// picture and the helper's login.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The column.
  static func details(_ snapshot: ViewerSnapshot) -> ViewerDetailsState {
    let reading = snapshot.reading.statusReading
    let holdingControl = snapshot.holdingControl
    let facts = reading.facts(holdingControl: holdingControl)
    return ViewerDetailsState(
      hostSession: hostSession(snapshot),
      helper: [
        Fact(name: ViewerWords.accountRow, value: account(snapshot.reading.status)),
        Fact(name: ViewerWords.stateRow, value: facts.input.word),
        Fact(name: Words.displayFact, value: Words.size(facts.display) ?? Words.unknown),
      ],
      ready: readyRows(reading.readinessItems),
      updates: updates(snapshot),
      newest: newest(snapshot),
      login: login(snapshot),
      isChecking: snapshot.session.checkingAgain
    )
  }

  /// Ready to act's rows: one, "Ready to act" with "All checks pass", while every check passes; else only the checks
  /// that do not, each with its mark and what it means (one not checked yet with its quiet ring), and a quiet count
  /// of those that do, with a green check. None before any check is known.
  ///
  /// - Parameter items: Each readiness check.
  /// - Returns: The rows.
  static func readyRows(_ items: [ReadinessItem]) -> [ViewerReadyRow] {
    guard !items.isEmpty else { return [] }
    let open = items.filter { $0.state != .ready }
    guard !open.isEmpty else {
      return [ViewerReadyRow(id: "all", name: ViewerWords.readyToAct, value: ViewerWords.allChecksPass, mark: .ready)]
    }
    var rows = open.map { item in
      ViewerReadyRow(id: item.name, name: item.name, mark: item.state, note: item.text.isEmpty ? nil : item.text)
    }
    let passing = items.count - open.count
    if passing > 0 {
      rows.append(ViewerReadyRow(id: "others", name: ViewerWords.othersPass(passing), mark: .ready, isQuiet: true))
    }
    return rows
  }

  /// The helper's login's rows, each only when it is known: when the helper logged in, and the agents connected to
  /// Atlas.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The rows; none when neither is known.
  static func login(_ snapshot: ViewerSnapshot) -> [Fact] {
    var rows: [Fact] = []
    if let login = snapshot.loggedInAt {
      rows.append(Fact(name: ViewerWords.loggedInRow, value: minute(login, snapshot)))
    }
    if let agents = snapshot.agents {
      let names = agents.map(Words.agentTitle)
      rows.append(
        Fact(name: ViewerWords.agentsRow, value: names.isEmpty ? ViewerWords.noAgents : names.joined(separator: ", "))
      )
    }
    return rows
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

  /// The chosen host session at the top of Details, while the sessions' list names it: its icon, name, state and
  /// line, its host and folder, when it started and was last seen or ended, the apps it opened as the Apps tab last
  /// saw them, the copies Atlas made for it, the plans it applied with where each stands and when it was made, its
  /// newest three screenshots, each labelled with its time, and cleaning up what it left. None while every
  /// session's work shows, or when the service offers no list.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The detail, or nil.
  static func hostSession(_ snapshot: ViewerSnapshot) -> ViewerSessionDetail? {
    guard case .one(let key) = snapshot.sessionChoice, let record = snapshot.panes.sessions.record(key) else {
      return nil
    }
    let reading = snapshot.reading.statusReading
    let named = reading.session(key)
    let listed = named?.isLive == true ? named : nil
    let state = listed?.state ?? (isEnded(record) ? .ended : .idle)
    let words =
      listed.map { ViewerWords.sessionSubtitle($0, line: reading.line(of: $0), now: snapshot.date) }
      ?? (isEnded(record)
        ? ViewerWords.endedSubtitle(record, now: snapshot.date)
        : ViewerWords.idle(record.lastSeenAt.map { snapshot.date.timeIntervalSince($0) }))
    let shots = record.screenshots
      .prefix(ViewerSessionDetail.mostShots)
      .map { name in
        ViewerActivityShot(name: name, thumbnail: snapshot.panes.thumbnails[name], label: shotLabel(name, snapshot))
      }
    return ViewerSessionDetail(
      icon: SessionIcons.icon(record.host),
      name: listed?.name ?? record.name,
      state: words,
      tone: state.tone,
      facts: sessionFacts(record, snapshot),
      apps: sessionApps(record, snapshot),
      copies: sessionCopies(record),
      plans: record.plans.map { plan in
        let made = plan.madeAt.map { ViewerWords.madeAt(minute($0, snapshot)) }
        return Fact(name: ViewerWords.planPhase(plan), value: made ?? "")
      },
      shots: Array(shots),
      allShots: record.screenshots.count > shots.count ? ViewerWords.seeAll(record.screenshots.count) : nil,
      cleanup: cleanup(record, snapshot)
    )
  }

  /// A session's facts: its host, or None named for Terminal; its folder when known; when it started; and when it
  /// ended, or was last seen.
  ///
  /// - Parameters:
  ///   - record: The session, as the list has it.
  ///   - snapshot: What the controller holds.
  /// - Returns: The facts.
  static func sessionFacts(_ record: HostSessionRecord, _ snapshot: ViewerSnapshot) -> [Fact] {
    let host =
      record.host == .terminal
      ? ViewerWords.noHost : Words.hostName(record.host, label: record.hostLabel, name: record.name)
    var facts = [Fact(name: ViewerWords.hostFact, value: host)]
    if let folder = record.folder { facts.append(Fact(name: ViewerWords.folderFact, value: folder)) }
    if let started = record.startedAt {
      facts.append(Fact(name: ViewerWords.startedFact, value: minute(started, snapshot)))
    }
    if let ended = record.endedAt {
      facts.append(Fact(name: ViewerWords.endedFact, value: Words.ago(snapshot.date.timeIntervalSince(ended))))
    } else if let seen = snapshot.reading.statusReading.session(record.key)?.lastSeenAt ?? record.lastSeenAt {
      facts.append(Fact(name: ViewerWords.lastSeenFact, value: Words.ago(snapshot.date.timeIntervalSince(seen))))
    }
    return facts
  }

  /// The apps a session opened: Open while the Apps tab's last reading lists it, "Open · a copy" when the session
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

  /// What VoiceOver reads for one of a session's screenshots in Details: when it was taken, as the screenshots list
  /// or its file name says, on the person's clock.
  ///
  /// - Parameters:
  ///   - name: The screenshot's file name.
  ///   - snapshot: What the controller holds.
  /// - Returns: "Screenshot taken at 10:31:44", or empty when its time is unknown.
  static func shotLabel(_ name: String, _ snapshot: ViewerSnapshot) -> String {
    let listed = snapshot.panes.shots.shots?.first { $0.name == name }?.takenAt
    guard let taken = listed ?? Screenshot.takenAt(named: name) else { return "" }
    return ViewerWords.shotLabel(clock(taken, snapshot))
  }

  /// How often the picture updates: Paused, Live or every five seconds.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The words.
  static func updates(_ snapshot: ViewerSnapshot) -> String {
    switch pictureChoice(snapshot) {
    case .live: ViewerWords.live
    case .everyFiveSeconds: ViewerWords.everyFiveSeconds
    case .paused: ViewerWords.paused
    }
  }

  /// The picture choice in force: paused, else the rate.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The choice.
  static func pictureChoice(_ snapshot: ViewerSnapshot) -> SettingsPicture {
    if snapshot.session.paused { return .paused }
    return snapshot.rate == .everyFiveSeconds ? .everyFiveSeconds : .live
  }
}
