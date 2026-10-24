import AtlasService
import Testing

@testable import AtlasApp

@Suite("AttentionSignals")
struct AttentionSignalsTests {
  /// A status whose overview is in `mode` and `state`.
  ///
  /// - Parameters:
  ///   - mode: The helper's mode.
  ///   - state: What every agent app shows.
  /// - Returns: The status.
  private func reading(_ mode: HelperMode, _ state: OverviewState) -> ViewerStatus {
    var status = ViewerStatus()
    status.overview.mode = mode
    status.overview.state = state
    return status
  }

  @Test(
    "nothing is posted while Notify me is off, a failed reading never tells twice, and turning Notify me on never tells of a state already there"
  )
  func notifyMe() {
    let log = SignalLog()
    var signals = log.signals
    let idle = reading(.observing, .idle)
    let needsYou = reading(.recovering, .needsYou)
    for status in [idle, needsYou] { signals.read(status) }
    #expect(log.posted.isEmpty, "Notify me is off")
    log.notify = true
    signals.read(needsYou)
    #expect(log.posted.isEmpty, "Needs you was already there")
    for status in [idle, needsYou, nil, needsYou] { signals.read(status) }
    #expect(log.posted == [.unfinished], "a failed reading in between is not the state clearing")
  }

  @Test("The helper needs you is withdrawn from Notification Center once Needs you clears, and only then")
  func needsYouIsWithdrawn() {
    let log = SignalLog()
    var signals = log.signals
    signals.read(reading(.recovering, .needsYou))
    signals.read(reading(.recovering, .needsYou))
    #expect(log.withdrawn == 0, "still needed")
    signals.read(nil)
    #expect(log.withdrawn == 0, "a failed reading is not the state clearing")
    signals.read(reading(.observing, .idle))
    #expect(log.withdrawn == 1)
    signals.read(reading(.observing, .idle))
    #expect(log.withdrawn == 1, "withdrawn once")
  }

  @Test("the helper logged out is withdrawn from Notification Center once it is logged in again, and only then")
  func loggedOutIsWithdrawn() {
    let log = SignalLog()
    var signals = log.signals
    signals.read(reading(.loggedOut, .notReady))
    signals.read(nil)
    #expect(log.withdrawnLoggedOut == 0, "a failed reading is not the helper coming back")
    signals.read(reading(.loggedOut, .notReady))
    #expect(log.withdrawnLoggedOut == 0, "still logged out")
    signals.read(reading(.observing, .idle))
    #expect(log.withdrawnLoggedOut == 1)
    signals.read(reading(.observing, .idle))
    #expect(log.withdrawnLoggedOut == 1, "withdrawn once")
  }
}
