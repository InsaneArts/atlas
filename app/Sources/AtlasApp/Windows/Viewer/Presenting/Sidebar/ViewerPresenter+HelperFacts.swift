import AtlasService
import AtlasStatus
import AtlasUI
import Foundation

extension ViewerPresenter {
  /// Every fact about the helper, as the helper line's tooltip lists them: its account, its state and its display;
  /// whether it is ready to act; how often the picture updates and how old the newest is; and, once read, when the
  /// helper logged in and the agents connected to Atlas.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The facts, in order.
  static func helperFacts(_ snapshot: ViewerSnapshot) -> [Fact] {
    let reading = snapshot.reading.statusReading
    let facts = reading.facts(holdingControl: snapshot.holdingControl)
    return [
      Fact(name: ViewerWords.accountRow, value: account(snapshot.reading.status)),
      Fact(name: ViewerWords.stateRow, value: facts.input.word),
      Fact(name: Words.displayFact, value: Words.size(facts.display) ?? Words.unknown),
    ] + readyFacts(reading.readinessItems)
      + [
        Fact(name: ViewerWords.updatesRow, value: updates(snapshot)),
        Fact(name: ViewerWords.newestRow, value: newest(snapshot)),
      ] + login(snapshot)
  }

  /// Whether the helper is ready to act: one fact, "Ready to act" with "All checks pass", while every check passes;
  /// else only the checks that do not, each with what it means, and a count of those that do. None before any check
  /// is known.
  ///
  /// - Parameter items: Each readiness check.
  /// - Returns: The facts.
  static func readyFacts(_ items: [ReadinessItem]) -> [Fact] {
    guard !items.isEmpty else { return [] }
    let open = items.filter { $0.state != .ready }
    guard !open.isEmpty else { return [Fact(name: ViewerWords.readyToAct, value: ViewerWords.allChecksPass)] }
    var facts = open.map { Fact(name: $0.name, value: $0.text) }
    let passing = items.count - open.count
    if passing > 0 { facts.append(Fact(name: ViewerWords.othersPass(passing), value: "")) }
    return facts
  }

  /// The helper's login's facts, each only when it is known: when the helper logged in, and the agents connected to
  /// Atlas.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The facts; none when neither is known.
  static func login(_ snapshot: ViewerSnapshot) -> [Fact] {
    var facts: [Fact] = []
    if let login = snapshot.loggedInAt {
      facts.append(Fact(name: ViewerWords.loggedInRow, value: minute(login, snapshot)))
    }
    if let agents = snapshot.agents {
      let names = agents.map(Words.agentTitle)
      facts.append(
        Fact(name: ViewerWords.agentsRow, value: names.isEmpty ? ViewerWords.noAgents : names.joined(separator: ", "))
      )
    }
    return facts
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
