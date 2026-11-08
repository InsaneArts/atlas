#if DEBUG
  import AtlasService
  import AtlasServiceTesting
  import AtlasUI
  import Foundation
  import Testing

  @testable import AtlasApp

  /// The panel's samples are written out by hand; these keep each one what the panel would show for the same facts.
  @Suite("PreviewStates+Panel")
  struct PanelPreviewStatesTests {
    /// What the controller holds on a set-up Mac whose helper is in `mode`: an agent's action in Opie, Atlas
    /// recovering the helper and the person's own click before it, a few seconds ago while it works and two minutes
    /// ago otherwise, Opie's icon found, and the helper's screen on show.
    ///
    /// - Parameters:
    ///   - mode: The helper's mode, as the service names it.
    ///   - state: The state the service shows for it.
    ///   - holdingControl: Whether Atlas's main window holds control.
    /// - Returns: The snapshot.
    /// - Throws: A decoding error, which a fixed reply never gives.
    func snapshot(_ mode: String, state: String, holdingControl: Bool = false) throws -> PanelSnapshot {
      let working = mode == "agent"
      let ages = working ? [2, 9, 31] : [120, 129, 151]
      let activity: [[String: Any]] = [
        [
          "operation": "input", "actor": "agent", "ok": true, "ageSeconds": ages[0],
          "says": working ? "turning Show Center off in Opie" : "turned Show Center off", "app": "Opie",
          "bundleId": SampleAppIcons.opie,
        ],
        ["operation": "restart", "actor": "agent", "ok": true, "ageSeconds": ages[1], "says": "recovered the helper"],
        [
          "operation": "input", "actor": "viewer", "ok": true, "ageSeconds": ages[2], "says": "clicked in Opie",
          "app": "Opie", "bundleId": SampleAppIcons.opie,
        ],
      ]
      var icons: [String: IconImage] = [:]
      if let key = AppIconKey(bundleId: SampleAppIcons.opie) {
        icons[key.text] = SampleAppIcons.icons[SampleAppIcons.opie]
      }
      return PanelSnapshot(
        status: try status(["overview": ["mode": mode, "state": state, "reason": "", "activity": activity]]),
        picture: SampleDesktop.picture,
        holdingControl: holdingControl,
        isOnConsole: true,
        icons: icons
      )
    }

    /// When the samples' questions were asked: 42 seconds before the snapshot.
    static let asked = Date(timeIntervalSince1970: 1_762_600_000)

    /// A question from the sample session k1 in Slack, unless another account is named.
    ///
    /// - Parameters:
    ///   - id: Its id.
    ///   - kind: What it would do.
    ///   - account: The account; Slack by default.
    ///   - element: The element's name; none by default.
    ///   - role: The element's role; none by default.
    ///   - typed: How many characters; none by default.
    ///   - message: The message; none by default.
    ///   - once: Whether Just This Once is offered; yes by default.
    ///   - frame: The element's frame on the display; none by default.
    /// - Returns: The question.
    static func ask(
      _ id: String,
      kind: Ask.Kind,
      account: String = "Slack",
      element: String? = nil,
      role: String? = nil,
      typed: Int = 0,
      message: Ask.Message? = nil,
      once: Bool = true,
      frame: Ask.Area? = nil
    ) -> Ask {
      Ask(
        id: id,
        session: "k1",
        who: "Claude Code · release",
        task: "post the 0.1.1 release notes",
        account: account,
        kind: kind,
        element: element,
        role: role,
        typed: typed,
        message: message,
        offersOnce: once,
        asked: asked,
        ends: asked + 120,
        picture: frame == nil ? nil : "\(id).jpg",
        frame: frame
      )
    }

    /// What the controller holds while an agent works and `ask` waits, 42 seconds after it was asked, the session
    /// that asked ended in the list, with its picture drawn from the sample chat app.
    ///
    /// - Parameters:
    ///   - ask: The question; none for none.
    ///   - picture: The id of its picture; none by default.
    /// - Returns: The snapshot.
    /// - Throws: A decoding error, which a fixed reply never gives.
    func asking(_ ask: Ask?, picture: String? = nil) throws -> PanelSnapshot {
      var snapshot = try snapshot("agent", state: "working")
      let session = hostSession(
        "k1",
        host: "claude",
        name: "Claude Code · release",
        state: "ended",
        hostLabel: "Claude Code"
      )
      snapshot.status.overview.hostSessions = try status(["overview": ["hostSessions": [session]]]).overview
        .hostSessions
      snapshot.status.overview.display = ViewerStatus.Size(width: 1440, height: 900)
      snapshot.date = Self.asked + 42
      snapshot.asks = AskList(asks: ask.map { [$0] } ?? [])
      if let ask, let name = ask.picture, let picture {
        snapshot.askPictures[name] = SampleDesktop.chat(picture, stage: .typed)
      }
      return snapshot
    }

    @Test("each panel sample is what the panel shows for the same facts")
    func samplesMatchThePresenter() throws {
      var paused = try snapshot("observing", state: "idle")
      paused.session.paused = true
      var refused = try snapshot("agent", state: "working")
      refused.session.notice = "The agent could not be stopped. Try again."
      var jobsLeft = try snapshot("stopped", state: "stopped")
      jobsLeft.session.notice = HelperWords.stopped(
        StopOutcome(warnings: [], refused: [StopOutcome.Job(id: "7c2e91a04b3f")], unresolved: [StopOutcome.Job(id: "")])
      )
      var controlCommand = try snapshot("controlling", state: "control", holdingControl: true)
      controlCommand.status.overview.commands = 1
      var needsYouCommand = try snapshot("recovering", state: "needsYou")
      needsYouCommand.status.overview.commands = 1
      var serviceNote = try snapshot("observing", state: "idle")
      serviceNote.status.restartingService = .restarting
      var accounts = try snapshot("observing", state: "idle")
      accounts.status.overview.accounts = [
        ViewerStatus.Account(kind: .site, key: "github.com", name: "github.com", added: 1),
        ViewerStatus.Account(kind: .app, key: "com.tinyspeck.slackmacgap", name: "Slack", added: 1),
      ]
      let message = Ask.Message(
        preview: "Release 0.1.1 is out: faster setup and fixes for typing in the helper.",
        length: 70
      )
      let send = Ask.Area(x: 1270, y: 710, width: 57, height: 59)
      let general = Ask.Area(x: 91, y: 264, width: 246, height: 44)
      var grant = try asking(nil)
      grant.asks = AskList(
        allowed: [
          AllowedGrant(
            session: "k1",
            account: AccountKey(kind: .site, id: "github.com"),
            name: "GitHub",
            ends: grant.date + 52 * 60 - 30
          )
        ]
      )
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
        "Stopped, jobs left": jobsLeft,
        "You have control, a command runs": controlCommand,
        "Needs you, a command runs": needsYouCommand,
        "Service note": serviceNote,
        "Accounts": accounts,
        "Waiting, a message": try asking(Self.ask("a1", kind: .sendKey, message: message)),
        "Waiting, a click": try asking(Self.ask("a2", kind: .click, element: "Send", frame: send), picture: "ask-a2"),
        "Waiting, a click with no name": try asking(
          Self.ask("a3", kind: .unnamed, role: "AXButton", frame: general),
          picture: "ask-a3"
        ),
        "Waiting, a password": try asking(Self.ask("a4", kind: .password, account: "github.com", typed: 12)),
        "Waiting, for this task or not": try asking(Self.ask("a5", kind: .sendKey, message: message, once: false)),
        "Allowed for a task": grant,
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
        "Switch": AlertSnapshot(alert: .switchToHelper, isOnConsole: true),
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
