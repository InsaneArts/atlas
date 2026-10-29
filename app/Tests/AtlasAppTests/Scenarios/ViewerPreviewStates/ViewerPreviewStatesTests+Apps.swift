#if DEBUG
  import AtlasService
  import AtlasServiceTesting
  import Foundation
  import Testing

  @testable import AtlasApp
  import AtlasUI

  extension ViewerPreviewStatesTests {
    /// The Apps tab's samples' facts, by the sample's name, read at 10:31:42 on a German clock in UTC.
    ///
    /// - Returns: What the presenter shows for each.
    /// - Throws: A decoding error, which a fixed reply never gives.
    func appsSamples() throws -> [String: ViewerState] {
      let idle = try reported("observing", state: "idle")
      let working = try sessionsReported(
        "agent",
        state: "working",
        [
          SessionReplies.claudeActing, SessionReplies.codexWaiting(), SessionReplies.cursorIdle,
          SessionReplies.terminalIdle,
        ]
      )
      let onApps = { (apps: [HelperApp], snapshot: inout ViewerSnapshot) in
        Self.onPlace(.apps, at: Self.activityReadAt, &snapshot)
        snapshot.panes.apps.apps = apps
        snapshot.panes.apps.sessions = Self.appSessions
        snapshot.panes.apps.steadyWindows = Set(apps.flatMap(\.windows).map(\.id))
        snapshot.panes.icons = Self.icons
      }
      let oneSession = { (apps: [HelperApp], snapshot: inout ViewerSnapshot) in
        Self.onSessionsActivity(Self.sessionsLog, &snapshot)
        snapshot.session.chosenSession = .one(Self.claudeKey)
        onApps(apps, &snapshot)
      }
      return [
        "Apps": shown(idle) { onApps(Self.apps, &$0) },
        "Apps, paused": shown(idle) { snapshot in
          onApps(Self.apps, &snapshot)
          snapshot.session.paused = true
        },
        "Apps, one session": shown(working) { oneSession(Self.apps, &$0) },
        "Apps, one session, none yet": shown(working) { snapshot in
          oneSession(Self.apps.filter { $0.origin?.session != Self.claudeKey }, &snapshot)
        },
        "Apps, a copy": shown(idle) { onApps(Self.copyApps, &$0) },
        "Apps, hidden": shown(idle) { onApps(Self.hiddenApps, &$0) },
        "Apps, many": shown(idle) { onApps(Self.manyApps, &$0) },
        "Show on screen": shown(idle) { snapshot in
          snapshot.reading.frame?.display = ViewerStatus.Size(width: 1728, height: 1080)
          snapshot.panes.icons = Self.icons
          snapshot.panes.apps.shown = ViewerShownWindow(
            app: "Opie",
            bundleId: SampleAppIcons.opie,
            window: HelperWindow(id: 12, title: "Ring settings", x: 1064, y: 80, width: 520, height: 860)
          )
        },
        "Show on screen, at the right edge": shown(idle) { snapshot in
          snapshot.reading.frame?.display = ViewerStatus.Size(width: 1728, height: 1080)
          snapshot.panes.icons = Self.icons
          snapshot.panes.apps.shown = ViewerShownWindow(
            app: "Opie",
            bundleId: SampleAppIcons.opie,
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
      ]
    }

    /// `seconds` before the samples are read, at 10:31:42.
    ///
    /// - Parameter seconds: How long before.
    /// - Returns: The time.
    static func beforeApps(_ seconds: TimeInterval) -> Date {
      activityReadAt.addingTimeInterval(-seconds)
    }

    /// A time of the samples' day, in UTC.
    ///
    /// - Parameters:
    ///   - hour: The hour.
    ///   - minute: The minute.
    /// - Returns: The time.
    static func appsClock(_ hour: Int, _ minute: Int) -> Date {
      let midnight = activityReadAt.addingTimeInterval(-(10 * 3600 + 31 * 60 + 42))
      return midnight.addingTimeInterval(TimeInterval(hour * 3600 + minute * 60))
    }

    /// The sessions the apps list names.
    static let appSessions = [
      claudeKey: HostSessionSummary(name: "Claude Code · understudy", host: .claude),
      codexKey: HostSessionSummary(name: "Codex · atlas-landing", host: .codex),
    ]

    /// Opie, run from Claude Code's copy 4 minutes ago and in front: its front window the sample desktop's one window.
    static let openOpie = HelperApp(
      pid: 501,
      name: "Opie",
      bundleId: SampleAppIcons.opie,
      front: true,
      launchedAt: beforeApps(240),
      origin: HelperAppOrigin(session: claudeKey, how: .copy, at: beforeApps(240)),
      windows: [
        HelperWindow(id: 12, title: "Ring settings", x: 230, y: 108, width: 806, height: 630, order: 0),
        HelperWindow(id: 11, title: "Starter", x: 300, y: 160, width: 1080, height: 590, order: 1),
      ]
    )

    /// Notes, which Codex opened 3 minutes ago, hidden.
    static let openNotes = HelperApp(
      pid: 503,
      name: "Notes",
      bundleId: SampleAppIcons.notes,
      hidden: true,
      launchedAt: beforeApps(180),
      origin: HelperAppOrigin(session: codexKey, how: .launch, at: beforeApps(180)),
      windows: [HelperWindow(id: 31, title: "QA checklist", x: 520, y: 200, width: 900, height: 640, order: 5)]
    )

    /// Probe, which one of Claude Code's tasks installed 11 minutes ago, its window behind Opie's.
    static let openProbe = HelperApp(
      pid: 506,
      name: "Probe",
      bundle: "/Users/atlas-helper/Applications/Probe.app",
      bundleId: "test.atlas.probe",
      launchedAt: beforeApps(660),
      origin: HelperAppOrigin(session: claudeKey, how: .install, at: beforeApps(660)),
      windows: [HelperWindow(id: 61, title: "Untitled", x: 160, y: 220, width: 640, height: 480, order: 3)]
    )

    /// Finder, open since 09:02 with no window.
    static let openFinder = HelperApp(
      pid: 505,
      name: "Finder",
      bundleId: SampleAppIcons.finder,
      launchedAt: appsClock(9, 2)
    )

    /// Safari, open since 09:12, its window behind Opie's.
    static let openSafari = HelperApp(
      pid: 502,
      name: "Safari",
      bundleId: SampleAppIcons.safari,
      launchedAt: appsClock(9, 12),
      windows: [
        HelperWindow(id: 21, title: "Opie — Release notes", x: 120, y: 80, width: 1200, height: 780, order: 2)
      ]
    )

    /// Terminal, open since 09:40, its window behind Opie's.
    static let openTerminal = HelperApp(
      pid: 504,
      name: "Terminal",
      bundleId: SampleAppIcons.terminal,
      launchedAt: appsClock(9, 40),
      windows: [HelperWindow(id: 41, title: "atlas — zsh", x: 680, y: 400, width: 720, height: 460, order: 4)]
    )

    /// The Apps sample's apps, front to back, as the service lists them.
    static let apps = [openOpie, openSafari, openProbe, openTerminal, openNotes, openFinder]

    /// A copy of a build only the helper can read, which Claude Code just opened and which is in front, beside Finder.
    static let copyApps = [
      HelperApp(
        pid: 601,
        name: "Probe",
        bundle: "/Users/atlas-helper/Library/Application Support/Atlas/Copies/Probe.app",
        bundleId: "test.atlas.probe-copy",
        front: true,
        launchedAt: beforeApps(20),
        origin: HelperAppOrigin(session: claudeKey, how: .copy, at: beforeApps(20)),
        windows: [HelperWindow(id: 71, title: "Probe", x: 230, y: 108, width: 806, height: 630, order: 0)]
      ),
      openFinder,
    ]

    /// Opie in front, with Notes and TextEdit hidden.
    static let hiddenApps = [
      openOpie,
      openNotes,
      HelperApp(
        pid: 507,
        name: "TextEdit",
        bundleId: SampleAppIcons.textEdit,
        hidden: true,
        launchedAt: appsClock(8, 55),
        windows: [
          HelperWindow(id: 71, title: "a.txt", x: 80, y: 90, width: 620, height: 520, order: 6),
          HelperWindow(id: 72, title: "Untitled", x: 760, y: 260, width: 560, height: 480, order: 7),
        ]
      ),
    ]

    /// Ten apps, Safari with five windows.
    static let manyApps: [HelperApp] = [
      openOpie,
      HelperApp(
        pid: 508,
        name: "Safari",
        bundleId: SampleAppIcons.safari,
        launchedAt: beforeApps(120),
        origin: HelperAppOrigin(session: codexKey, how: .launch, at: beforeApps(120)),
        windows: [
          HelperWindow(id: 81, title: "Opie — Release notes", x: 120, y: 80, width: 1200, height: 780, order: 2),
          HelperWindow(id: 82, title: "Ring settings guide", x: 160, y: 120, width: 1100, height: 720, order: 3),
          HelperWindow(id: 83, title: "Pricing", x: 200, y: 160, width: 1000, height: 680, order: 4),
          HelperWindow(id: 84, title: "Changelog", x: 240, y: 200, width: 900, height: 640, order: 5),
          HelperWindow(id: 85, title: "Support", x: 280, y: 240, width: 800, height: 600, order: 6),
        ]
      ),
      openProbe,
      HelperApp(
        pid: 509,
        name: "BuildTool",
        bundle: "/Users/atlas-helper/Applications/BuildTool.app",
        launchedAt: appsClock(9, 20),
        origin: HelperAppOrigin(session: claudeKey, how: .install, at: appsClock(9, 20)),
        windows: [HelperWindow(id: 91, title: "BuildTool", x: 400, y: 300, width: 560, height: 400, order: 7)]
      ),
      openTerminal,
      openNotes,
      HelperApp(
        pid: 511,
        name: "Calendar",
        launchedAt: appsClock(8, 41),
        windows: [HelperWindow(id: 92, title: "Calendar", x: 300, y: 140, width: 900, height: 600, order: 8)]
      ),
      HelperApp(
        pid: 507,
        name: "TextEdit",
        bundleId: SampleAppIcons.textEdit,
        launchedAt: appsClock(8, 55),
        windows: [HelperWindow(id: 71, title: "a.txt", x: 80, y: 90, width: 620, height: 520, order: 9)]
      ),
      HelperApp(pid: 510, name: "Mail", launchedAt: appsClock(8, 40)),
      openFinder,
    ]

    /// The icons found for the Apps sample's apps, by their keys: every app's but the copy's.
    static var icons: ViewerIconsFacts {
      var icons = ViewerIconsFacts()
      for (id, icon) in SampleAppIcons.icons {
        if let key = AppIconKey(bundleId: id) { icons.images[key.text] = icon }
      }
      return icons
    }
  }
#endif
