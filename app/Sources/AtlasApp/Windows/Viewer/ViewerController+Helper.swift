import AtlasUI
import Foundation

/// The helper's facts that its status does not carry: the agents connected to Atlas and when the helper logged in,
/// which the helper line's tooltip lists. They are read once the window first shows, the login time again whenever
/// the helper's account changes, and both by Check Again, never with every reading. Check Again reads the status too.
extension ViewerController {
  /// Reads the agents connected to Atlas and the helper's login time. Only the newest reading can replace the agents;
  /// a reading that fails keeps what was known.
  func readHelperFacts() async {
    helperFactsGeneration += 1
    let generation = helperFactsGeneration
    readLoginTime()
    if let read = await dependencies.connectedAgents(), generation == helperFactsGeneration {
      helperFacts.agents = read
    }
  }

  /// Reads when the helper's account logged in, once its account is known.
  func readLoginTime() {
    guard let account = reading.status.overview.account else { return }
    helperFacts.loggedInAt = dependencies.loginTime(account)
  }

  /// Check Again: reads the status and the helper's own facts now, then the next picture at once; once at a time,
  /// its item waiting meanwhile. It holds for at least ``AtlasUI/Motion/Duration/leastWork`` on the window's clock, so
  /// a check that answers at once still reads as checked rather than a flicker.
  func checkAgain() async {
    guard !session.checkingAgain else { return }
    session.checkingAgain = true
    defer { session.checkingAgain = false }
    let clock = dependencies.clock
    // Unstructured, so the least time runs beside the readings, not after them.
    let least = Task { try? await clock.sleep(for: .seconds(Motion.Duration.leastWork.seconds)) }
    await readingLoop.readStatus()
    readingLoop.refresh()
    await readHelperFacts()
    await least.value
  }
}
