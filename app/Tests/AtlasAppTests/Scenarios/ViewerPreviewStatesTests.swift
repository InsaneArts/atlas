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
    /// An agent's latest actions, as the boards show them, newest first.
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
    /// - Returns: The decoded status.
    /// - Throws: A decoding error, which a fixed reply never gives.
    func reported(
      _ mode: String,
      state: String,
      activity: [[String: Any]] = earlier,
      checks: [[String: Any]] = [
        ["name": "Helper running", "ok": true], ["name": "Input service ready", "ok": true],
      ],
      dialogs: [String: Any] = ["state": "passed", "ageSeconds": 600]
    ) throws -> ViewerStatus {
      let reply: [String: Any] = [
        "ready": true,
        "canObserve": true,
        "checks": checks,
        "session": ["user": ["name": "Atlas"], "inputService": ["ok": true, "ready": true]],
        "capabilities": ["nativeDialogs": dialogs],
        "overview": [
          "mode": mode,
          "state": state,
          "reason": "The helper's reason",
          "account": "atlas",
          "accountName": "Atlas",
          "display": ["width": 1440, "height": 900],
          "pending": mode == "recovering" ? 1 : 0,
          "activity": activity,
        ],
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
      let idle = try reported("observing", state: "idle")
      let loggedOut = try reported("loggedOut", state: "notReady")
      let gone = "Atlas isn't answering in the helper."
      return [
        "Working": shown(try reported("agent", state: "working", activity: Self.working)),
        "Idle": shown(idle),
        "You have control": shown(try reported("controlling", state: "control")) { $0.session.lease = "lease" },
        "Needs you": shown(try reported("recovering", state: "needsYou")),
        "Stopped": shown(try reported("stopped", state: "stopped")),
        "In use elsewhere": shown(try reported("controlling", state: "control")),
        "Paused": shown(idle) { $0.session.paused = true },
        "Details": shown(idle) {
          $0.detailsShown = true
          $0.agents = [.claude, .codex]
          // 09:12 in UTC, told as a German clock tells it, as the board does.
          $0.loggedInAt = Date(timeIntervalSince1970: 9 * 3600 + 12 * 60)
          $0.timeZone = TimeZone(secondsFromGMT: 0) ?? .current
          $0.locale = Locale(identifier: "de_DE")
        },
        "Details, some checks not passing": shown(
          try reported(
            "observing",
            state: "idle",
            checks: [
              ["name": "Helper running", "ok": true],
              ["name": "Input service ready", "ok": false, "message": "Accessibility is off"],
            ],
            dialogs: ["state": "unchecked", "reason": "Not run since the helper started"]
          )
        ) {
          $0.detailsShown = true
          $0.agents = [.claude, .codex]
          $0.loggedInAt = Date(timeIntervalSince1970: 9 * 3600 + 12 * 60)
          $0.timeZone = TimeZone(secondsFromGMT: 0) ?? .current
          $0.locale = Locale(identifier: "de_DE")
        },
        "Not ready": shown(try reported("unavailable", state: "notReady"), picture: false, unavailable: gone),
        "Logged out": shown(loggedOut, picture: false, unavailable: gone),
        "Connecting": ViewerPresenter.state(ViewerSnapshot()),
        "Switch": shown(loggedOut, picture: false, unavailable: gone) { $0.session.alert = .switchToHelper },
        "Log out": shown(idle) { $0.session.alert = .logOut },
        "Log out anyway": shown(idle) { $0.session.alert = .logOutAnyway },
        "Restart": shown(idle) { $0.session.alert = .restart },
        "Typing": shown(try reported("controlling", state: "control")) {
          $0.session.lease = "lease"
          $0.session.composing = "こんにちは"
        },
        "Mid-action": shown(try reported("agent", state: "working", activity: Self.working)) {
          $0.session.notice = ViewerWords.agentMidAction
        },
        "Actual size, paused": shown(idle) {
          $0.session.paused = true
          $0.session.actualSize = true
        },
        "Actual size, needs you": shown(try reported("recovering", state: "needsYou")) { $0.session.actualSize = true },
      ]
      .merging(try presentedTabs()) { _, tab in tab }
    }

    /// The tabs' samples' facts, by the sample's name: times on a German clock in UTC, as the boards tell them.
    ///
    /// - Returns: What the presenter shows for each.
    /// - Throws: A decoding error, which a fixed reply never gives.
    func presentedTabs() throws -> [String: ViewerState] {
      let working = try reported("agent", state: "working", activity: Self.working)
      let idle = try reported("observing", state: "idle")
      let shotStatus = try reported("observing", state: "idle", activity: Self.tookAScreenshot)
      return [
        "Activity": shown(working) { snapshot in
          Self.onTab(.activity, at: Self.activityReadAt, &snapshot)
          snapshot.panes.activity.log = Self.activityLog
          snapshot.panes.activity.readAt = Self.activityReadAt
          snapshot.panes.shots.shots = Self.shots
          Self.keepThumbnails(&snapshot)
        },
        "Activity, nothing yet": shown(try reported("observing", state: "idle", activity: [])) { snapshot in
          Self.onTab(.activity, at: Self.activityReadAt, &snapshot)
          snapshot.panes.activity.log = ActivityLog()
          snapshot.panes.activity.readAt = Self.activityReadAt
        },
        "Activity, paused": shown(idle) { snapshot in
          Self.onTab(.activity, at: Self.activityReadAt, &snapshot)
          snapshot.session.paused = true
          snapshot.panes.activity.log = Self.activityLog
          snapshot.panes.activity.readAt = Self.activityReadAt
          snapshot.panes.shots.shots = Self.shots
          Self.keepThumbnails(&snapshot)
        },
        "Apps, paused": shown(idle) { snapshot in
          snapshot.session.tab = .apps
          snapshot.session.paused = true
          snapshot.panes.apps.apps = Self.apps
          snapshot.panes.apps.icons = SampleAppIcons.icons
        },
        "Apps": shown(idle) { snapshot in
          snapshot.session.tab = .apps
          snapshot.panes.apps.apps = Self.apps
          snapshot.panes.apps.icons = SampleAppIcons.icons
        },
        "Show on screen": shown(idle) { snapshot in
          snapshot.reading.frame?.display = ViewerStatus.Size(width: 1728, height: 1080)
          snapshot.panes.apps.icons = SampleAppIcons.icons
          snapshot.panes.apps.shown = ViewerShownWindow(
            app: "Opie",
            icon: SampleAppIcons.opie,
            window: HelperWindow(id: 12, title: "Ring settings", x: 1064, y: 80, width: 520, height: 860)
          )
        },
        "Show on screen, at the right edge": shown(idle) { snapshot in
          snapshot.reading.frame?.display = ViewerStatus.Size(width: 1728, height: 1080)
          snapshot.panes.apps.icons = SampleAppIcons.icons
          snapshot.panes.apps.shown = ViewerShownWindow(
            app: "Opie",
            icon: SampleAppIcons.opie,
            window: HelperWindow(
              id: 13,
              title: "Ring settings for the appearance of every ring and slice",
              x: 1428,
              y: 80,
              width: 300,
              height: 600
            )
          )
        },
        "Screenshots": shown(shotStatus) { snapshot in
          Self.onShots(&snapshot)
        },
        "A screenshot, large": shown(shotStatus) { snapshot in
          Self.onShots(&snapshot)
          let name = PreviewStates.shotNames[1]
          snapshot.panes.shots.large = name
          snapshot.panes.shots.full = ViewerFullShot(
            png: Data(),
            picture: SampleDesktop.picture.map { DesktopPicture(id: "\(name) full", image: $0.image) }
          )
        },
        "Screenshots, none yet": shown(shotStatus) { snapshot in
          Self.onTab(.screenshots, at: Self.shotsNow, &snapshot)
          snapshot.panes.shots.shots = []
        },
      ]
    }

    /// The last thing an agent did: a screenshot two minutes ago.
    static let tookAScreenshot: [[String: Any]] = [
      ["operation": "screenshot", "says": "took a screenshot", "ok": true, "ageSeconds": 120]
    ]

    /// 10:31:42 on 22 October 2026, in UTC: when the Activity sample's log is read.
    static let activityReadAt = Date(timeIntervalSince1970: 1_792_665_102)

    /// 11:13:00 on 22 October 2026, in UTC: when the Screenshots sample is looked at, an hour after 10:12.
    static let shotsNow = Date(timeIntervalSince1970: 1_792_667_580)

    /// The Activity sample's log, newest first (MacActivity), read at 10:31:42.
    static let activityLog = ActivityLog(entries: [
      ActivityLog.Entry(id: "38", says: "turning Show Center off in Opie", outcome: .running, ageSeconds: 0),
      ActivityLog.Entry(id: "37", says: "scrolled in Opie", ageSeconds: 4),
      ActivityLog.Entry(id: "36", says: "clicked Ring in Opie", ageSeconds: 9),
      ActivityLog.Entry(id: "35", says: "took a screenshot", ageSeconds: 22, operation: "screenshot"),
      ActivityLog.Entry(id: "34", says: "opened Opie", ageSeconds: 30, operation: "open"),
      ActivityLog.Entry(id: "33", says: "gave back control", actor: .you, ageSeconds: 122, operation: "release"),
      ActivityLog.Entry(id: "32", says: "clicked in Opie", actor: .you, ageSeconds: 140),
      ActivityLog.Entry(id: "31", says: "clicked in Opie", actor: .you, ageSeconds: 150),
      ActivityLog.Entry(id: "30", says: "clicked in Opie", actor: .you, ageSeconds: 157),
      ActivityLog.Entry(
        id: "29",
        says: "couldn't click Show Centre in Opie",
        outcome: .failed,
        ageSeconds: 192,
        reason: "No element is named Show Centre"
      ),
      ActivityLog.Entry(id: "28", says: "ran a command", ageSeconds: 224, operation: "run"),
    ])

    /// The Screenshots sample's captures, newest first, each 2560 × 1440.
    static let shots: [Screenshot] = zip(
      PreviewStates.shotNames,
      [1_792_665_105, 1_792_665_104, 1_792_665_080, 1_792_665_074, 1_792_663_923, 1_792_663_121]
    )
    .map { Screenshot(name: $0, takenAt: Date(timeIntervalSince1970: $1), width: 2560, height: 1440) }

    /// The Apps sample's apps, front to back (MacApps).
    static let apps = [
      HelperApp(
        pid: 501,
        name: "Opie",
        bundle: SampleAppIcons.opie,
        front: true,
        windows: [
          HelperWindow(id: 11, title: "Starter", width: 1080, height: 590),
          HelperWindow(id: 12, title: "Ring settings", width: 520, height: 860),
        ]
      ),
      HelperApp(
        pid: 502,
        name: "Safari",
        bundle: SampleAppIcons.safari,
        windows: [HelperWindow(id: 21, title: "Opie — Release notes", width: 1200, height: 780)]
      ),
      HelperApp(
        pid: 503,
        name: "Notes",
        bundle: SampleAppIcons.notes,
        windows: [HelperWindow(id: 31, title: "QA checklist", width: 900, height: 640)]
      ),
      HelperApp(
        pid: 504,
        name: "Terminal",
        bundle: SampleAppIcons.terminal,
        windows: [HelperWindow(id: 41, title: "atlas — zsh", width: 720, height: 460)]
      ),
      HelperApp(pid: 505, name: "Finder", bundle: SampleAppIcons.finder),
    ]

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

    /// The Screenshots tab with the sample's captures, 10:31:44 selected and every thumbnail read.
    ///
    /// - Parameter snapshot: The facts to change.
    static func onShots(_ snapshot: inout ViewerSnapshot) {
      onTab(.screenshots, at: shotsNow, &snapshot)
      snapshot.panes.shots.shots = shots
      snapshot.panes.shots.folder = URL(fileURLWithPath: "/Users/me/runs", isDirectory: true)
      snapshot.panes.shots.selected = PreviewStates.shotNames[1]
      keepThumbnails(&snapshot)
    }

    /// Every capture's thumbnail read: the sample desktop, known by the capture's name.
    ///
    /// - Parameter snapshot: The facts to change.
    static func keepThumbnails(_ snapshot: inout ViewerSnapshot) {
      guard let sample = SampleDesktop.picture else { return }
      for name in PreviewStates.shotNames {
        snapshot.panes.thumbnails.keep(DesktopPicture(id: name, image: sample.image), for: name)
      }
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
