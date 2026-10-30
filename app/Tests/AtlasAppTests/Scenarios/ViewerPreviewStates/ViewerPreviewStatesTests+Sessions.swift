#if DEBUG
  import AtlasService
  import AtlasServiceTesting
  import Foundation
  import Testing

  @testable import AtlasApp
  import AtlasUI

  extension ViewerPreviewStatesTests {
    /// The host sessions' samples' facts, by the sample's name: Claude Code · understudy with the helper, Codex ·
    /// atlas-landing waiting, Cursor · opie idle and a Terminal one, as the panel's samples list them; times on a German
    /// clock in UTC, for deterministic timestamps.
    ///
    /// - Returns: What the presenter shows for each.
    /// - Throws: A decoding error, which a fixed reply never gives.
    func sessionsSamples() throws -> [String: ViewerState] {
      let working = try sessionsReported(
        "agent",
        state: "working",
        [
          SessionReplies.claudeActing, SessionReplies.codexWaiting(), SessionReplies.cursorIdle,
          SessionReplies.terminalIdle,
        ]
      )
      let needsYou = try sessionsReported(
        "recovering",
        state: "needsYou",
        [SessionReplies.claudeNeedsYou, SessionReplies.codexWaiting(recovery: true), SessionReplies.cursorIdle],
        activity: Self.unfinished
      )
      let idle = try reported("observing", state: "idle")
      let detail = { (snapshot: inout ViewerSnapshot) in
        snapshot.date = Self.activityReadAt
        snapshot.timeZone = TimeZone(secondsFromGMT: 0) ?? .current
        snapshot.locale = Locale(identifier: "de_DE")
        snapshot.session.chosenSession = .one(Self.claudeKey)
        snapshot.panes.sessions.list = HostSessionList(sessions: [Self.claudeRecord])
        // Opie, run from the session's copy, so the Library counts it among the session's apps.
        snapshot.panes.apps.apps = [
          HelperApp(pid: 501, name: "Opie", front: true, origin: HelperAppOrigin(session: Self.claudeKey, how: .copy))
        ]
        Self.keepThumbnails(&snapshot)
      }
      return [
        "Sessions": shown(working),
        "Sessions, one ended": shown(working) { snapshot in
          snapshot.date = Self.activityReadAt
          snapshot.panes.sessions.list = HostSessionList(sessions: [Self.piRecord])
        },
        "Activity by session": shown(working) { snapshot in
          Self.onSessionsActivity(Self.sessionsLog, &snapshot)
        },
        "Activity, needs you": shown(needsYou) { snapshot in
          Self.onSessionsActivity(Self.needsYouLog, &snapshot)
          // The person set the recovery alert aside with Not Now, to look at what each session did first.
          if let offered = snapshot.offeredRecovery {
            snapshot.session.recoveryLater = RecoveryKey(offered, status: snapshot.reading.status)
          }
        },
        "Activity, one session": shown(working) { snapshot in
          Self.onSessionsActivity(Self.sessionsLog, &snapshot)
          snapshot.session.chosenSession = .one(Self.claudeKey)
        },
        "Session chosen": shown(working, detail),
        "Clean up": shown(working) { snapshot in
          detail(&snapshot)
          snapshot.session.alert = .cleanup(
            session: Self.claudeKey,
            steps: ViewerPresenter.cleanupSteps(Self.cleanupPreview),
            warning: ViewerPresenter.cleanupWarning(Self.cleanupPreview)
          )
        },
        "Session cleaned up": shown(working) { snapshot in
          detail(&snapshot)
          var record = Self.claudeRecord
          record.plans = []
          record.installs = ["Probe"]
          record.cleanup = .incomplete
          snapshot.panes.sessions.list = HostSessionList(sessions: [record])
          snapshot.session.cleanup.receipts[Self.claudeKey] = Self.cleanupApplied
          // Said as the window's controller says it once the cleanup ran.
          snapshot.session.notice = ViewerPresenter.cleanupNotice(Self.cleanupApplied)
          snapshot.session.noticeAction = snapshot.reading.statusReading.activity(limit: 1).first?.id
        },
        "Activity, one session, nothing yet": shown(working) { snapshot in
          Self.onSessionsActivity(ActivityLog(), &snapshot)
          snapshot.session.chosenSession = .one(Self.claudeKey)
        },
        "Screenshots, one session": shown(working) { snapshot in
          Self.onSessionsActivity(Self.sessionsLog, &snapshot)
          Self.onShots(&snapshot)
          snapshot.session.chosenSession = .one(Self.claudeKey)
          snapshot.panes.sessions.list = HostSessionList(sessions: [Self.claudeRecord])
        },
        "Screenshots, one session, none yet": shown(working) { snapshot in
          Self.onSessionsActivity(Self.sessionsLog, &snapshot)
          Self.onShots(&snapshot)
          snapshot.session.chosenSession = .one(Self.claudeKey)
        },
        "Session cleaning up": shown(working) { snapshot in
          detail(&snapshot)
          snapshot.session.cleanup.running = Self.claudeKey
        },
        "Session ended": shown(working) { snapshot in
          detail(&snapshot)
          snapshot.session.chosenSession = .one("c0ffee5e5510")
          snapshot.panes.sessions.list = HostSessionList(sessions: [Self.piRecord])
        },
        "Sidebar by state": shown(try byState()) { snapshot in
          Self.byState(&snapshot)
          snapshot.session.endedShown = true
        },
        "Sidebar, Ended folded": shown(try byState()) { snapshot in
          Self.byState(&snapshot)
        },
        "Sidebar, search open": shown(working) { snapshot in
          snapshot.session.search = ViewerSearchState(isOpen: true, query: "opie")
        },
        "Sidebar hidden": shown(working) { snapshot in
          snapshot.date = Self.activityReadAt
          snapshot.panes.sessions.list = HostSessionList(sessions: [Self.piRecord])
          snapshot.session.sidebarHidden = true
        },
        "Focus changed": shown(idle) { snapshot in
          Self.onPlace(.activity, at: Self.activityReadAt, &snapshot)
          snapshot.panes.activity.log = Self.focusLog
          snapshot.panes.activity.readAt = Self.activityReadAt
          snapshot.panes.icons = Self.icons
        },
      ]
    }

    /// The helper needing the person while four sessions are live: Claude Code's click unfinished, Codex waiting until
    /// the person recovers, and Cursor and Terminal idle.
    ///
    /// - Returns: The status.
    /// - Throws: A decoding error, which a fixed reply never gives.
    func byState() throws -> ViewerStatus {
      try sessionsReported(
        "recovering",
        state: "needsYou",
        [
          SessionReplies.claudeNeedsYou, SessionReplies.codexWaiting(recovery: true), SessionReplies.cursorIdle,
          SessionReplies.terminalIdle,
        ],
        activity: Self.unfinished
      )
    }

    /// The sidebar by state's own facts: Pi · notes-sync ended, and the recovery alert set aside with Not Now.
    ///
    /// - Parameter snapshot: The facts to change.
    static func byState(_ snapshot: inout ViewerSnapshot) {
      snapshot.date = activityReadAt
      snapshot.timeZone = TimeZone(secondsFromGMT: 0) ?? .current
      snapshot.locale = Locale(identifier: "de_DE")
      snapshot.panes.sessions.list = HostSessionList(sessions: [piRecord])
      if let offered = snapshot.offeredRecovery {
        snapshot.session.recoveryLater = RecoveryKey(offered, status: snapshot.reading.status)
      }
    }

    /// What cleaning up Claude Code's work takes while it still acts: quitting Opie, rolling back its plan, moving
    /// Probe, which its task installed, aside in the helper and removing the copy of Opie.
    static let cleanupPreview = CleanupReply(
      ok: true,
      preview: true,
      steps: [
        CleanupStep(number: 1, kind: .quit, apps: ["Opie"]),
        CleanupStep(number: 2, kind: .undoPlan),
        CleanupStep(number: 3, kind: .removeInstall, app: "Probe", unchanged: true),
        CleanupStep(number: 4, kind: .removeCopy, app: "Opie"),
      ],
      active: true
    )

    /// That cleanup once it ran: Opie quit and the plan rolled back, Probe kept as it changed since it was installed,
    /// and the copy of Opie still in use.
    static let cleanupApplied = CleanupReply(
      state: .incomplete,
      steps: [
        CleanupStep(number: 1, kind: .quit, apps: ["Opie"], state: .done),
        CleanupStep(number: 2, kind: .undoPlan, state: .done),
        CleanupStep(number: 3, kind: .removeInstall, app: "Probe", state: .kept, reason: .changed),
        CleanupStep(number: 4, kind: .removeCopy, app: "Opie", state: .failed, reason: .inUse, retry: true),
      ]
    )

    /// Claude Code's latest actions, newest first, as the status lists them while it has the helper.
    static let clicking: [[String: Any]] = [
      ["operation": "input", "says": "clicking Show Center in Opie", "ok": true, "started": true, "ageSeconds": 0],
      ["operation": "input", "says": "scrolled in Opie", "ok": true, "ageSeconds": 5],
      ["operation": "input", "says": "clicked Ring in Opie", "ok": true, "ageSeconds": 9],
    ]

    /// Claude Code's latest actions while its click may not have finished.
    static let unfinished: [[String: Any]] = [
      [
        "id": "c1", "operation": "input", "says": "clicked Show Center in Opie", "ok": false, "uncertain": true,
        "ageSeconds": 2,
      ],
      ["operation": "input", "says": "scrolled in Opie", "ok": true, "ageSeconds": 5],
      ["operation": "input", "says": "clicked Ring in Opie", "ok": true, "ageSeconds": 9],
    ]

    /// A status while these host sessions are listed: the helper in `mode`, sessions shown, the session holding the
    /// helper named, and Claude Code's latest actions.
    ///
    /// - Parameters:
    ///   - mode: The helper's mode.
    ///   - state: The state the service shows for it.
    ///   - sessions: The entries, in the service's order.
    ///   - activity: Claude Code's latest actions, newest first; clicking in Opie by default.
    /// - Returns: The decoded status.
    /// - Throws: A decoding error, which a fixed reply never gives.
    func sessionsReported(
      _ mode: String,
      state: String,
      _ sessions: [[String: Any]],
      activity: [[String: Any]] = clicking
    ) throws -> ViewerStatus {
      var fields: [String: Any] = ["sessionsShown": true, "hostSessions": sessions]
      fields["holder"] = sessions.first { $0["holdsHelper"] as? Bool == true }?["key"]
      let rows = activity.map { row in
        row.merging(["session": Self.claudeKey]) { _, session in session }
      }
      return try reported(mode, state: state, activity: rows, overviewFields: fields)
    }

    /// Claude Code's session key, as the panel's samples have it.
    static let claudeKey = "3f9a1c07b2e4"
    /// Codex's session key.
    static let codexKey = "8be20c4d1a77"
    /// Cursor's session key.
    static let cursorKey = "5d21e9a04c3f"

    /// The Activity tab over `log`, read at 10:31:42, with the screenshots and their thumbnails.
    ///
    /// - Parameters:
    ///   - log: The log read.
    ///   - snapshot: The facts to change.
    static func onSessionsActivity(_ log: ActivityLog, _ snapshot: inout ViewerSnapshot) {
      onPlace(.activity, at: activityReadAt, &snapshot)
      snapshot.panes.activity.log = log
      snapshot.panes.activity.readAt = activityReadAt
      snapshot.panes.shots.shots = shots
      snapshot.panes.icons = icons
      keepThumbnails(&snapshot)
    }

    /// Claude Code's rows, newest first: clicking now, then what it did before, the oldest earlier today.
    static let claudeRows = [
      row(
        "38",
        "clicking Show Center in Opie",
        age: 0,
        verb: "click",
        app: opie,
        outcome: .running,
        label: "Show Center",
        session: claudeKey
      ),
      row("37", "scrolled in Opie", age: 4, verb: "scroll", app: opie, session: claudeKey),
      row("36", "clicked Ring in Opie", age: 9, verb: "click", app: opie, label: "Ring", session: claudeKey),
      row(
        "35",
        "took a screenshot",
        age: 22,
        verb: "screenshot",
        app: nil,
        operation: "screenshot",
        session: claudeKey
      ),
      row("34", "opened Opie", age: 30, verb: "open", app: opie, operation: "open", session: claudeKey),
    ]

    /// The rows of Codex, the person, Cursor and Terminal, newest first, after Claude Code's.
    static let otherRows = [
      row(
        "x2",
        "didn't type in Notes",
        age: 100,
        verb: "type",
        app: notes,
        outcome: .notSent,
        reason: "Another app was in front",
        session: codexKey
      ),
      row("x1", "opened Notes", age: 112, verb: "open", app: notes, operation: "open", session: codexKey),
      ActivityLog.Entry(
        id: "33",
        says: "gave back control",
        actor: .you,
        ageSeconds: 122,
        operation: "release"
      ),
      persons("32", age: 140),
      persons("31", age: 150),
      persons("30", age: 157),
      row("u1", "opened Opie", age: 240, verb: "open", app: opie, operation: "open", session: cursorKey),
      row("t1", "ran a command", age: 300, verb: "run", app: nil, operation: "run"),
      row("27", "ran a command", age: 400, verb: "run", app: nil, operation: "run", session: claudeKey),
    ]

    /// One of the person's own clicks in Opie, through the viewer.
    ///
    /// - Parameters:
    ///   - id: The row's id.
    ///   - age: How long before the reading it started.
    /// - Returns: The row.
    static func persons(_ id: String, age: Double) -> ActivityLog.Entry {
      ActivityLog.Entry(
        id: id,
        says: "clicked in Opie",
        actor: .you,
        ageSeconds: age,
        verb: "click",
        app: "Opie",
        bundleId: SampleAppIcons.opie
      )
    }

    /// The log while every session works: Claude Code's five rows, Codex's two, the person's, Cursor's and Terminal's.
    static let sessionsLog = ActivityLog(entries: claudeRows + otherRows)

    /// The log while Claude Code's click may not have finished: its click, whose outcome is unknown, in place of what
    /// it was doing.
    static let needsYouLog = ActivityLog(
      entries: [
        row(
          "c1",
          "clicked Show Center in Opie",
          age: 2,
          verb: "click",
          app: opie,
          outcome: .failed,
          reason: "Its outcome is unknown",
          session: claudeKey
        )
      ] + claudeRows.dropFirst() + otherRows
    )

    /// The log of one agent whose typing was refused because another app was in front, then typed.
    static let focusLog = ActivityLog(entries: [
      row("f3", "typed in Notes", age: 2, verb: "type", app: notes),
      row(
        "f2",
        "didn't type in Notes",
        age: 4,
        verb: "type",
        app: notes,
        outcome: .notSent,
        reason: "Another app was in front"
      ),
      row("f1", "opened Notes", age: 12, verb: "open", app: notes, operation: "open"),
    ])

    /// Pi · notes-sync's record: it started at 09:40 and ended 20 minutes before Activity was read.
    static let piRecord = HostSessionRecord(
      key: "c0ffee5e5510",
      name: "Pi · notes-sync",
      host: .pi,
      folder: "notes-sync",
      startedAt: Date(timeIntervalSince1970: 1_792_662_000),
      endedAt: activityReadAt.addingTimeInterval(-20 * 60),
      state: .ended
    )

    /// Claude Code's session as `sessions.json` lists it: started at 09:12, seen two seconds ago, with Opie opened
    /// from a copy, Notes opened, a plan applied at 10:02, and six screenshots.
    static let claudeRecord = HostSessionRecord(
      key: Self.claudeKey,
      name: "Claude Code · understudy",
      host: .claude,
      hostLabel: "Claude Code",
      folder: "understudy",
      startedAt: Date(timeIntervalSince1970: 1_792_660_320),
      lastSeenAt: Date(timeIntervalSince1970: 1_792_665_100),
      actions: 14,
      apps: [HostSessionApp(name: "Opie", copied: true), HostSessionApp(name: "Notes")],
      copies: [HostSessionCopy(app: "Opie")],
      plans: [HostSessionPlan(phase: .applied, madeAt: Date(timeIntervalSince1970: 1_792_663_320), canRollBack: true)],
      screenshots: PreviewStates.shotNames
    )
  }
#endif
