import AtlasUI
import Foundation

extension ViewerPresenter {
  /// Details: the helper, its readiness checks, the picture and the session.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The column.
  static func details(_ snapshot: ViewerSnapshot) -> ViewerDetailsState {
    let reading = snapshot.reading.statusReading
    let holdingControl = snapshot.holdingControl
    let facts = reading.facts(holdingControl: holdingControl)
    return ViewerDetailsState(
      helper: [
        Fact(name: ViewerWords.accountRow, value: account(snapshot.reading.status)),
        Fact(name: ViewerWords.stateRow, value: facts.input.word),
        Fact(name: Words.displayFact, value: Words.size(facts.display) ?? Words.unknown),
      ],
      ready: readyRows(reading.readinessItems),
      updates: updates(snapshot),
      newest: newest(snapshot),
      session: session(snapshot),
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

  /// The session's rows, each only when it is known: when the helper logged in, and the agents connected to Atlas.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The rows; none when neither is known.
  static func session(_ snapshot: ViewerSnapshot) -> [Fact] {
    var rows: [Fact] = []
    if let login = snapshot.loggedInAt {
      // The hour in two digits, such as 09:12, on the person's own 12- or 24-hour clock.
      let style = Date.FormatStyle(locale: snapshot.locale, timeZone: snapshot.timeZone)
        .hour(.twoDigits(amPM: .abbreviated))
        .minute(.twoDigits)
      rows.append(Fact(name: ViewerWords.loggedInRow, value: login.formatted(style)))
    }
    if let agents = snapshot.agents {
      let names = agents.map(Words.agentTitle)
      rows.append(
        Fact(name: ViewerWords.agentsRow, value: names.isEmpty ? ViewerWords.noAgents : names.joined(separator: ", "))
      )
    }
    return rows
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
