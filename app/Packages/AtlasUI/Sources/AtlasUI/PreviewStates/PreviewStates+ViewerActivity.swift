#if DEBUG
  extension PreviewStates {
    /// The Activity sample's lines, newest first, read at 10:31:42.
    @MainActor static let activityGroups = [
      ViewerActivityGroup(
        id: "now",
        title: "Now",
        lines: [
          ViewerActivityLine(
            id: "38",
            time: "10:31:42",
            lead: .live,
            kind: .pointer,
            text: "Turning Show Center off in Opie",
            label: "10:31:42, Turning Show Center off in Opie, now"
          ),
          ViewerActivityLine(
            id: "37",
            time: "10:31:38",
            lead: .quiet,
            kind: .scroll,
            text: "Scrolled in Opie",
            label: "10:31:38, Scrolled in Opie"
          ),
          ViewerActivityLine(
            id: "36",
            time: "10:31:33",
            lead: .quiet,
            kind: .pointer,
            text: "Clicked Ring in Opie",
            label: "10:31:33, Clicked Ring in Opie"
          ),
          ViewerActivityLine(
            id: "35",
            time: "10:31:20",
            lead: .quiet,
            kind: .screenshot,
            text: "Took a screenshot",
            shot: ViewerActivityShot(name: shotNames[2], thumbnail: shotPicture(shotNames[2])),
            label: "10:31:20, Took a screenshot"
          ),
          ViewerActivityLine(
            id: "34",
            time: "10:31:12",
            lead: .quiet,
            kind: .window,
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
            kind: .control,
            text: "You gave back control",
            label: "10:29:40, You gave back control"
          ),
          ViewerActivityLine(
            id: "30",
            time: "10:29:05",
            lead: .person,
            kind: .person,
            text: "You clicked in Opie ×3",
            label: "10:29:05, You clicked in Opie ×3"
          ),
          ViewerActivityLine(
            id: "29",
            time: "10:28:30",
            lead: .failed,
            kind: .problem,
            text: "Couldn't click Show Centre in Opie",
            reason: "No element is named Show Centre",
            label: "10:28:30, Couldn't click Show Centre in Opie, No element is named Show Centre"
          ),
          ViewerActivityLine(
            id: "28",
            time: "10:27:58",
            lead: .quiet,
            kind: .command,
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

    /// The Activity tab while an agent works, with the live screen in the corner.
    @MainActor public static let viewerActivity: ViewerState = {
      var state = viewerWorking
      // Off the Screen no caption says what the agent is doing, so the line beside the word does.
      state.top.status.line = "Turning Show Center off in Opie"
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
      state.top.status = StatusSummary(label: "Idle", tone: .quiet, mark: .dot, line: "Ready for your agents")
      state.top.tabs = viewerTabs(.activity)
      state.mini = SampleDesktop.picture
        .map { MiniScreenState(picture: $0) }
      state.pane = .activity(
        ViewerActivityState(
          meta: "",
          query: "",
          filters: activityFilters(),
          groups: [],
          empty: EmptyMessageState(
            headline: "Nothing has happened yet",
            message: "What agents and you do in the helper shows up here."
          ),
          isSearchable: false
        )
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
  }
#endif
