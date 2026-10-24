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

  @Test("the badge shows only while Needs you lasts, the badge setting is on and the Dock icon shows")
  func badge() {
    let log = SignalLog()
    var signals = log.signals
    signals.read(reading(.observing, .idle))
    signals.read(reading(.recovering, .needsYou))
    signals.dockIcon(shown: true)
    #expect(log.badges.isEmpty, "the badge setting is off")
    log.badge = true
    signals.badgeChanged()
    #expect(log.badges == [NotificationWords.badge])
    signals.dockIcon(shown: false)
    signals.dockIcon(shown: true)
    signals.read(reading(.observing, .idle))
    #expect(log.badges == [NotificationWords.badge, nil, NotificationWords.badge, nil])
  }

  @Test("the badge waits for input that stays stopped, so a helper logging in never badges the Dock")
  func badgeWaitsForStoppedInput() {
    let log = SignalLog()
    log.badge = true
    var signals = log.signals
    signals.dockIcon(shown: true)
    signals.read(reading(.observing, .idle))
    signals.read(reading(.blocked, .needsYou))
    #expect(log.badges.isEmpty, "input may still start by itself")
    signals.read(reading(.blocked, .needsYou))
    #expect(log.badges == [NotificationWords.badge])
  }
}
