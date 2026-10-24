#if DEBUG
  import AtlasService
  import AtlasServiceTesting
  import Foundation
  import Testing

  @testable import AtlasApp
  import AtlasUI

  /// The main window's samples are written out by hand; these keep each one what the window would show for the same
  /// facts.
  @Suite("PreviewStates+Viewer")
  struct ViewerPreviewStatesTests {
    /// An agent's latest actions, in the sample log, newest first.
    static let working: [[String: Any]] = [
      ["operation": "input", "says": "turning Show Center off in Opie", "ok": true, "ageSeconds": 2],
      ["operation": "input", "says": "scrolled in Opie", "ok": true, "ageSeconds": 5],
      ["operation": "input", "says": "clicked Ring in Opie", "ok": true, "ageSeconds": 9],
    ]

    /// What an agent did last, long enough ago that no caption shows it.
    static let earlier: [[String: Any]] = [
      ["operation": "input", "says": "turned Show Center off", "ok": true, "ageSeconds": 120]
    ]

    /// A status as the viewer service reports it, every check ready.
    ///
    /// - Parameters:
    ///   - mode: The helper's mode.
    ///   - state: The state the service shows for it.
    ///   - activity: Its recent activity, newest first.
    ///   - checks: Its checks; every one passing by default.
    ///   - dialogs: How its Save and Open dialogs check stands; passed by default.
    ///   - reason: The helper's reason, as the overview gives it; nil for none.
    /// - Returns: The decoded status.
    /// - Throws: A decoding error, which a fixed reply never gives.
    func reported(
      _ mode: String,
      state: String,
      activity: [[String: Any]] = earlier,
      checks: [[String: Any]] = [
        ["name": "Helper running", "ok": true], ["name": "Input service ready", "ok": true],
      ],
      dialogs: [String: Any] = ["state": "passed", "ageSeconds": 600],
      reason: String? = "The helper's reason"
    ) throws -> ViewerStatus {
      var overview: [String: Any] = [
        "mode": mode,
        "state": state,
        "account": "atlas",
        "accountName": "Atlas",
        "display": ["width": 1440, "height": 900],
        "pending": mode == "recovering" ? 1 : 0,
        "activity": activity,
      ]
      overview["reason"] = reason
      let reply: [String: Any] = [
        "ready": true,
        "canObserve": true,
        "checks": checks,
        "session": ["user": ["name": "Atlas"], "inputService": ["ok": true, "ready": true]],
        "capabilities": ["nativeDialogs": dialogs],
        "overview": overview,
      ]
      return try status(reply)
    }

    /// What the presenter shows for a status and the window's own facts.
    ///
    /// - Parameters:
    ///   - status: What the service reports.
    ///   - picture: Whether the helper's screen shows.
    ///   - unavailable: Why there is no picture, if there is none.
    ///   - change: The window's own facts, changed from a window that only watches.
    /// - Returns: The state.
    func shown(
      _ status: ViewerStatus,
      picture: Bool = true,
      unavailable: String? = nil,
      _ change: (inout ViewerSnapshot) -> Void = { _ in }
    ) -> ViewerState {
      var snapshot = ViewerSnapshot()
      snapshot.reading.status = status
      snapshot.reading.hasStatus = true
      snapshot.reading.ready = true
      snapshot.reading.unavailable = unavailable
      if picture, let sample = SampleDesktop.picture {
        snapshot.reading.picture = sample
        snapshot.reading.frame = Frame(
          png: Data(),
          id: sample.id,
          revision: sample.id,
          display: ViewerStatus.Size(width: 1440, height: 900)
        )
        snapshot.reading.frameAt = snapshot.now
      }
      change(&snapshot)
      return ViewerPresenter.state(snapshot)
    }

    /// Each sample's facts, by the sample's name.
    ///
    /// - Returns: What the presenter shows for each.
    /// - Throws: A decoding error, which a fixed reply never gives.
    func presented() throws -> [String: ViewerState] {
      try screenSamples()
        .merging(try activitySamples()) { _, tab in tab }
        .merging(try appsSamples()) { _, tab in tab }
        .merging(try shotsSamples()) { _, tab in tab }
    }

    /// Shows `tab` at `date`, on a German clock in UTC.
    ///
    /// - Parameters:
    ///   - tab: The tab.
    ///   - date: The time now.
    ///   - snapshot: The facts to change.
    static func onTab(_ tab: ViewerTab, at date: Date, _ snapshot: inout ViewerSnapshot) {
      snapshot.session.tab = tab
      snapshot.date = date
      snapshot.timeZone = TimeZone(secondsFromGMT: 0) ?? .current
      snapshot.locale = Locale(identifier: "de_DE")
    }

    @Test("each main window sample is what the window shows for the same facts")
    func samplesMatchThePresenter() throws {
      let presented = try presented()
      #expect(Set(presented.keys) == Set(PreviewStates.viewer.map(\.name)), "every sample has its facts, and no more")
      for preview in PreviewStates.viewer {
        let state = try #require(presented[preview.name], "\(preview.name) has no facts")
        #expect(preview.value == state, "\(preview.name)")
      }
    }
  }
#endif
