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
        snapshot.detailsShown = true
        snapshot.agents = [.claude, .codex]
        snapshot.loggedInAt = Date(timeIntervalSince1970: 9 * 3600 + 12 * 60)
        snapshot.session.sessionChoice = .one(Self.claudeKey)
        snapshot.panes.sessions.list = HostSessionList(sessions: [Self.claudeRecord])
        snapshot.panes.apps.apps = [HelperApp(pid: 501, name: "Opie", front: true)]
        Self.keepThumbnails(&snapshot)
      }
      return [
        "Sessions": shown(working),
        "Sessions, menu open": shown(working) { $0.session.openMenu = .sessions },
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
          snapshot.session.sessionChoice = .one(Self.claudeKey)
        },
        "Session detail": shown(working, detail),
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
          snapshot.session.cleanupReceipts[Self.claudeKey] = Self.cleanupApplied
        },
        "Focus changed": shown(idle) { snapshot in
          Self.onTab(.activity, at: Self.activityReadAt, &snapshot)
          snapshot.panes.activity.log = Self.focusLog
          snapshot.panes.activity.readAt = Self.activityReadAt
        },
      ]
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
        CleanupStep(number: 3, kind: .removeInstall, app: "Probe", state: .kept, reason: "changed"),
        CleanupStep(number: 4, kind: .removeCopy, app: "Opie", state: .failed, reason: "in-use", retry: true),
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
      onTab(.activity, at: activityReadAt, &snapshot)
      snapshot.panes.activity.log = log
      snapshot.panes.activity.readAt = activityReadAt
      snapshot.panes.shots.shots = shots
      keepThumbnails(&snapshot)
    }

    /// Claude Code's rows, newest first: clicking now, then what it did before, the oldest earlier today.
    static let claudeRows = [
      ActivityLog.Entry(
        id: "38",
        says: "clicking Show Center in Opie",
        outcome: .running,
        ageSeconds: 0,
        session: Self.claudeKey
      ),
      ActivityLog.Entry(id: "37", says: "scrolled in Opie", ageSeconds: 4, session: Self.claudeKey),
      ActivityLog.Entry(id: "36", says: "clicked Ring in Opie", ageSeconds: 9, session: Self.claudeKey),
      ActivityLog.Entry(
        id: "35",
        says: "took a screenshot",
        ageSeconds: 22,
        operation: "screenshot",
        session: Self.claudeKey,
        capture: PreviewStates.shotNames[2]
      ),
      ActivityLog.Entry(
        id: "34",
        says: "opened Opie",
        ageSeconds: 30,
        operation: "open",
        session: Self.claudeKey
      ),
    ]

    /// The rows of Codex, the person, Cursor and Terminal, newest first, after Claude Code's.
    static let otherRows = [
      ActivityLog.Entry(
        id: "x2",
        says: "didn't type in Notes",
        outcome: .notSent,
        ageSeconds: 100,
        reason: "Another app was in front",
        session: Self.codexKey
      ),
      ActivityLog.Entry(
        id: "x1",
        says: "opened Notes",
        ageSeconds: 112,
        operation: "open",
        session: Self.codexKey
      ),
      ActivityLog.Entry(id: "33", says: "gave back control", actor: .you, ageSeconds: 122, operation: "release"),
      ActivityLog.Entry(id: "32", says: "clicked in Opie", actor: .you, ageSeconds: 140),
      ActivityLog.Entry(id: "31", says: "clicked in Opie", actor: .you, ageSeconds: 150),
      ActivityLog.Entry(id: "30", says: "clicked in Opie", actor: .you, ageSeconds: 157),
      ActivityLog.Entry(
        id: "u1",
        says: "opened Opie",
        ageSeconds: 240,
        operation: "open",
        session: Self.cursorKey
      ),
      ActivityLog.Entry(id: "t1", says: "ran a command", ageSeconds: 300, operation: "run"),
      ActivityLog.Entry(
        id: "27",
        says: "ran a command",
        ageSeconds: 400,
        operation: "run",
        session: Self.claudeKey
      ),
    ]

    /// The log while every session works: Claude Code's six rows, Codex's two, the person's, Cursor's and Terminal's.
    static let sessionsLog = ActivityLog(entries: claudeRows + otherRows)

    /// The log while Claude Code's click may not have finished: its click, whose outcome is unknown, in place of what
    /// it was doing.
    static let needsYouLog = ActivityLog(
      entries: [
        ActivityLog.Entry(
          id: "c1",
          says: "clicked Show Center in Opie",
          outcome: .failed,
          ageSeconds: 2,
          reason: "Its outcome is unknown",
          session: Self.claudeKey
        )
      ] + claudeRows.dropFirst() + otherRows
    )

    /// The log of one agent whose typing was refused because another app was in front, then typed.
    static let focusLog = ActivityLog(entries: [
      ActivityLog.Entry(id: "f3", says: "typed in Notes", ageSeconds: 2),
      ActivityLog.Entry(
        id: "f2",
        says: "didn't type in Notes",
        outcome: .notSent,
        ageSeconds: 4,
        reason: "Another app was in front"
      ),
      ActivityLog.Entry(id: "f1", says: "opened Notes", ageSeconds: 12, operation: "open"),
    ])

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
