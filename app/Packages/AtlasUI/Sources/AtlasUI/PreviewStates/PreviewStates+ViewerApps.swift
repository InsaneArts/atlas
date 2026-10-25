#if DEBUG
  import CoreGraphics

  extension PreviewStates {
    /// One of the Apps sample's windows.
    ///
    /// - Parameters:
    ///   - id: The window's number.
    ///   - title: Its title.
    ///   - size: Its size.
    /// - Returns: The row.
    private static func window(_ id: Int, _ title: String, _ size: String) -> ViewerWindowRow {
      ViewerWindowRow(id: id, title: title, size: size, showLabel: "Show \(title) on the screen")
    }

    /// The Apps sample's apps, front to back.
    @MainActor static let sampleApps = [
      ViewerAppRow(
        id: 501,
        name: "Opie",
        icon: SampleAppIcons.icons[SampleAppIcons.opie],
        isFront: true,
        windows: [window(11, "Starter", "1080 × 590"), window(12, "Ring settings", "520 × 860")]
      ),
      ViewerAppRow(
        id: 502,
        name: "Safari",
        icon: SampleAppIcons.icons[SampleAppIcons.safari],
        windows: [window(21, "Opie — Release notes", "1200 × 780")]
      ),
      ViewerAppRow(
        id: 503,
        name: "Notes",
        icon: SampleAppIcons.icons[SampleAppIcons.notes],
        windows: [window(31, "QA checklist", "900 × 640")]
      ),
      ViewerAppRow(
        id: 504,
        name: "Terminal",
        icon: SampleAppIcons.icons[SampleAppIcons.terminal],
        windows: [window(41, "atlas — zsh", "720 × 460")]
      ),
      ViewerAppRow(id: 505, name: "Finder", icon: SampleAppIcons.icons[SampleAppIcons.finder], windows: []),
    ]

    /// The Apps tab while the helper is idle, with the live screen in the corner.
    @MainActor public static let viewerApps: ViewerState = {
      var state = viewerIdle
      state.top.tabs = viewerTabs(.apps)
      state.mini = SampleDesktop.picture
        .map { MiniScreenState(picture: $0) }
      state.pane = .apps(ViewerAppsState(meta: "5 apps · 5 windows", query: "", apps: sampleApps))
      return state
    }()

    /// The Apps tab while the picture is paused: the screen in the corner fades and says so.
    @MainActor public static let viewerAppsPaused: ViewerState = {
      var state = viewerPaused
      state.top.tabs = viewerTabs(.apps)
      state.mini = SampleDesktop.picture
        .map { MiniScreenState(picture: $0, isPaused: true) }
      state.pane = viewerApps.pane
      return state
    }()

    /// Opie's Ring settings window outlined on the screen, from the Apps tab: 520 × 860 at 1064, 80 on a 1728
    /// × 1080 display.
    @MainActor public static let viewerShowOnScreen: ViewerState = {
      var state = viewerIdle
      state.top.status = StatusSummary(label: "Idle", tone: .quiet, mark: .dot, line: "Showing Opie's window")
      state.screen.outline = ViewerWindowOutline(
        frame: CGRect(
          x: 1064.0 / 1728,
          y: 80.0 / 1080,
          width: 1584.0 / 1728 - 1064.0 / 1728,
          height: 940.0 / 1080 - 80.0 / 1080
        ),
        title: "Opie · Ring settings",
        size: "520 × 860",
        icon: SampleAppIcons.icons[SampleAppIcons.opie]
      )
      return state
    }()

    /// A window docked at the right edge with a long name, outlined: its label keeps inside the picture and cuts the
    /// name short. 300 × 600 at 1428, 80 on a 1728 × 1080 display.
    @MainActor public static let viewerShowOnScreenRightEdge: ViewerState = {
      var state = viewerShowOnScreen
      state.screen.outline = ViewerWindowOutline(
        frame: CGRect(
          x: 1428.0 / 1728,
          y: 80.0 / 1080,
          width: 1728.0 / 1728 - 1428.0 / 1728,
          height: 680.0 / 1080 - 80.0 / 1080
        ),
        title: "Opie · Ring settings for the appearance of every ring and slice",
        size: "300 × 600",
        icon: SampleAppIcons.icons[SampleAppIcons.opie]
      )
      return state
    }()
  }
#endif
