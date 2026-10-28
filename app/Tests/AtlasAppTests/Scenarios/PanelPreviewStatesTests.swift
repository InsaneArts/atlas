#if DEBUG
  import AtlasService
  import AtlasServiceTesting
  import AtlasUI
  import Testing

  @testable import AtlasApp

  /// The panel's samples are written out by hand; these keep each one what the panel would show for the same facts.
  @Suite("PreviewStates+Panel")
  struct PanelPreviewStatesTests {
    /// What the controller holds on a set-up Mac whose helper is in `mode`: an agent's three actions in Opie, a few
    /// seconds ago while it works and two minutes ago otherwise, and the helper's screen on show.
    ///
    /// - Parameters:
    ///   - mode: The helper's mode, as the service names it.
    ///   - state: The state the service shows for it.
    ///   - holdingControl: Whether Atlas's main window holds control.
    /// - Returns: The snapshot.
    /// - Throws: A decoding error, which a fixed reply never gives.
    func snapshot(_ mode: String, state: String, holdingControl: Bool = false) throws -> PanelSnapshot {
      let working = mode == "agent"
      let said = [
        working ? "turning Show Center off in Opie" : "turned Show Center off", "clicked Ring in Opie", "opened Opie",
      ]
      let ages = working ? [2, 9, 31] : [120, 129, 151]
      let activity = zip(said, ages)
        .map { ["operation": "input", "actor": "agent", "ok": true, "ageSeconds": $1, "says": $0] as [String: Any] }
      return PanelSnapshot(
        status: try status(["overview": ["mode": mode, "state": state, "reason": "", "activity": activity]]),
        picture: SampleDesktop.picture,
        holdingControl: holdingControl
      )
    }

    @Test("each panel sample is what the panel shows for the same facts")
    func samplesMatchThePresenter() throws {
      var paused = try snapshot("observing", state: "idle")
      paused.session.paused = true
      var refused = try snapshot("agent", state: "working")
      refused.session.notice = "The agent could not be stopped. Try again."
      let presented: [String: PanelSnapshot] = [
        "Idle": try snapshot("observing", state: "idle"),
        "Working": try snapshot("agent", state: "working"),
        "You have control": try snapshot("controlling", state: "control", holdingControl: true),
        "Needs you": try snapshot("recovering", state: "needsYou"),
        "Logged out": try snapshot("loggedOut", state: "notReady"),
        "Not ready": try snapshot("unavailable", state: "notReady"),
        "Stopped": try snapshot("stopped", state: "stopped"),
        "Connecting": PanelSnapshot(connecting: true),
        "In use elsewhere": try snapshot("controlling", state: "control"),
        "Paused": paused,
        "Stop refused": refused,
        "Sessions": try SessionReplies.snapshot([SessionReplies.claudeActing, SessionReplies.cursorIdle]),
        "Sessions waiting": try SessionReplies.snapshot([
          SessionReplies.claudeActing, SessionReplies.codexWaiting(), SessionReplies.cursorIdle,
        ]),
        "Sessions need you": try SessionReplies.snapshot(
          [SessionReplies.claudeNeedsYou, SessionReplies.codexWaiting(recovery: true), SessionReplies.cursorIdle],
          mode: "recovering",
          state: "needsYou",
          pending: 1
        ),
        "Sessions, more than three": try SessionReplies.snapshot([
          SessionReplies.claudeActing, SessionReplies.codexWaiting(), SessionReplies.cursorIdle,
          SessionReplies.terminalIdle, SessionReplies.piIdle,
        ]),
      ]
      #expect(Set(presented.keys) == Set(PreviewStates.panel.map(\.name)), "every sample has its facts, and no more")
      for preview in PreviewStates.panel {
        let snapshot = try #require(presented[preview.name], "\(preview.name) has no facts")
        #expect(preview.value == PanelPresenter.state(snapshot), "\(preview.name)")
      }
    }

    @Test("each alert window sample is what the alert window shows for the same alert")
    func alertSamplesMatchThePresenter() {
      var recovering = ViewerStatus.Overview()
      recovering.mode = .recovering
      let presented: [String: AlertSnapshot] = [
        "Recover": AlertSnapshot(alert: .recover, overview: recovering),
        "Switch": AlertSnapshot(alert: .switchToHelper),
        "Log out": AlertSnapshot(alert: .logOut),
        "Log out anyway": AlertSnapshot(alert: .logOutAnyway),
        "Recover refused": AlertSnapshot(alert: .recover, failure: HelperWords.recoverBusy, overview: recovering),
        "Recovering": AlertSnapshot(alert: .recover, running: true, overview: recovering),
        "Recovered": AlertSnapshot(alert: .recover, running: true, done: true, overview: recovering),
        "Logging out": AlertSnapshot(alert: .logOut, running: true),
      ]
      #expect(Set(presented.keys) == Set(PreviewStates.alert.map(\.name)), "every sample has its facts, and no more")
      for preview in PreviewStates.alert {
        guard let snapshot = presented[preview.name] else { continue }
        #expect(preview.value == AlertPresenter.state(snapshot), "\(preview.name)")
      }
    }
  }
#endif
