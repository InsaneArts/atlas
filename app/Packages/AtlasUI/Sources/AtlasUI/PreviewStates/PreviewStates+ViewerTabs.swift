#if DEBUG
  import CoreGraphics

  extension PreviewStates {
    /// The Activity sample's lines, as MacActivity lists them, read at 10:31:42.
    @MainActor static let activityGroups = [
      ViewerActivityGroup(
        id: "now",
        title: "Now",
        lines: [
          ViewerActivityLine(
            id: "38",
            time: "10:31:42",
            lead: .live,
            text: "Turning Show Center off in Opie",
            label: "10:31:42, Turning Show Center off in Opie, now"
          ),
          ViewerActivityLine(
            id: "37",
            time: "10:31:38",
            lead: .quiet,
            text: "Scrolled in Opie",
            label: "10:31:38, Scrolled in Opie"
          ),
          ViewerActivityLine(
            id: "36",
            time: "10:31:33",
            lead: .quiet,
            text: "Clicked Ring in Opie",
            label: "10:31:33, Clicked Ring in Opie"
          ),
          ViewerActivityLine(
            id: "35",
            time: "10:31:20",
            lead: .quiet,
            text: "Took a screenshot",
            shot: ViewerActivityShot(name: shotNames[2], thumbnail: shotPicture(shotNames[2])),
            label: "10:31:20, Took a screenshot"
          ),
          ViewerActivityLine(
            id: "34",
            time: "10:31:12",
            lead: .quiet,
            text: "Opened Opie",
            label: "10:31:12, Opened Opie"
          ),
        ]
      ),
      ViewerActivityGroup(
        id: "today",
        title: "Earlier today",
        lines: [
          ViewerActivityLine(
            id: "33",
            time: "10:29:40",
            lead: .person,
            text: "You gave back control",
            label: "10:29:40, You gave back control"
          ),
          ViewerActivityLine(
            id: "30",
            time: "10:29:05",
            lead: .person,
            text: "You clicked in Opie ×3",
            label: "10:29:05, You clicked in Opie ×3"
          ),
          ViewerActivityLine(
            id: "29",
            time: "10:28:30",
            lead: .failed,
            text: "Couldn't click Show Centre in Opie",
            reason: "No element is named Show Centre",
            label: "10:28:30, Couldn't click Show Centre in Opie, No element is named Show Centre"
          ),
          ViewerActivityLine(
            id: "28",
            time: "10:27:58",
            lead: .quiet,
            text: "Ran a command",
            label: "10:27:58, Ran a command"
          ),
        ]
      ),
    ]

    /// The Activity pane's filters, one of them chosen.
    ///
    /// - Parameter selected: The filter chosen.
    /// - Returns: The filters.
    static func activityFilters(_ selected: ViewerActivityFilter = .all) -> CapsuleTabsState<ViewerActivityFilter> {
      CapsuleTabsState(
        tabs: [
          CapsuleTab(id: .all, title: "All"),
          CapsuleTab(id: .agent, title: "Agent"),
          CapsuleTab(id: .you, title: "You"),
          CapsuleTab(id: .problems, title: "Problems"),
        ],
        selected: selected,
        label: "Which actions"
      )
    }

    /// The Activity tab while an agent works, with the live screen in the corner (MacActivity).
    @MainActor public static let viewerActivity: ViewerState = {
      var state = viewerWorking
      state.top.tabs = viewerTabs(.activity)
      state.mini = SampleDesktop.picture
        .map { MiniScreenState(picture: $0) }
      state.pane = .activity(
        ViewerActivityState(meta: "11 actions today", query: "", filters: activityFilters(), groups: activityGroups)
      )
      return state
    }()

    /// The Activity tab before anything was done in the helper.
    @MainActor public static let viewerActivityEmpty: ViewerState = {
      var state = viewerIdle
      state.top.status = StatusSummary(label: "Idle", tone: .quiet, mark: .dot, line: "")
      state.top.tabs = viewerTabs(.activity)
      state.mini = SampleDesktop.picture
        .map { MiniScreenState(picture: $0) }
      state.pane = .activity(
        ViewerActivityState(
          meta: "No actions today",
          query: "",
          filters: activityFilters(),
          groups: [],
          empty: EmptyMessageState(
            headline: "Nothing has happened yet",
            message: "What agents and you do in the helper shows up here."
          )
        )
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

    /// The Activity tab while the picture is paused: the screen in the corner fades and says so.
    @MainActor public static let viewerActivityPaused: ViewerState = {
      var state = viewerPaused
      state.top.tabs = viewerTabs(.activity)
      state.mini = SampleDesktop.picture
        .map { MiniScreenState(picture: $0, isPaused: true) }
      state.pane = viewerActivity.pane
      return state
    }()

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

    /// The Apps sample's apps, front to back (MacApps).
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

    /// The Apps tab while the helper is idle, with the live screen in the corner (MacApps).
    @MainActor public static let viewerApps: ViewerState = {
      var state = viewerIdle
      state.top.tabs = viewerTabs(.apps)
      state.mini = SampleDesktop.picture
        .map { MiniScreenState(picture: $0) }
      state.pane = .apps(ViewerAppsState(meta: "5 apps · 5 windows, front to back", query: "", apps: sampleApps))
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

    /// Opie's Ring settings window outlined on the screen, from the Apps tab (MacShowOnScreen): 520 × 860 at 1064, 80
    /// on a 1728 × 1080 display.
    @MainActor public static let viewerShowOnScreen: ViewerState = {
      var state = viewerIdle
      state.top.status = StatusSummary(label: "Idle", tone: .quiet, mark: .dot, line: "Showing Opie's window")
      state.top.actions.showsClear = true
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
  }
#endif
