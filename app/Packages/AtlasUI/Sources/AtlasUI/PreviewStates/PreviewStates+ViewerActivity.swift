#if DEBUG
  import Foundation

  extension PreviewStates {
    /// The Activity pane's filters, one of them chosen, Problems and Screenshots with their counts.
    ///
    /// - Parameters:
    ///   - selected: The filter chosen.
    ///   - problems: How many problems; none drawn for zero.
    ///   - screenshots: How many screenshots; none drawn for zero.
    /// - Returns: The filters.
    static func activityFilters(
      _ selected: ViewerActivityFilter = .all,
      problems: Int = 0,
      screenshots: Int = 0
    ) -> CapsuleTabsState<ViewerActivityFilter> {
      CapsuleTabsState(
        tabs: [
          CapsuleTab(id: .all, title: "All"),
          CapsuleTab(id: .problems, title: "Problems", count: problems > 0 ? problems : nil),
          CapsuleTab(id: .screenshots, title: "Screenshots", count: screenshots > 0 ? screenshots : nil),
          CapsuleTab(id: .typing, title: "Typing"),
        ],
        selected: selected,
        label: "Which actions"
      )
    }

    /// An app's icon in a sample: its stand-in icon when the samples draw one, else its tile.
    ///
    /// - Parameters:
    ///   - id: The app's bundle identifier, if known.
    ///   - name: Its name.
    /// - Returns: What leads its moment.
    @MainActor static func sampleApp(_ id: String?, _ name: String) -> ActivityIcon {
      .app(.of(id.flatMap { SampleAppIcons.icons[$0] }, name: name))
    }

    /// A screenshot's step's screenshot in a sample, 16:9 as the samples' captures are, its thumbnail read.
    ///
    /// - Parameter name: The capture's file name.
    /// - Returns: The screenshot.
    @MainActor static func sampleShot(_ name: String) -> ViewerActivityShot {
      ViewerActivityShot(name: name, aspect: Self.shotAspect, thumbnail: shotPicture(name))
    }

    /// 16:9: the samples' captures' shape, 2560 by 1440.
    static let shotAspect: CGFloat = 2560.0 / 1440.0

    /// Activity while an agent works, with the live screen in the corner: Opie open with the step running, Notes
    /// open with the typing refused and done again, TextEdit and an older command folded.
    @MainActor public static let viewerActivity: ViewerState = {
      var state = viewerWorking
      state.sidebar.library = viewerLibrary(.activity, counts: [.activity: 19, .screenshots: 6])
      state.mini = SampleDesktop.picture
        .map { MiniScreenState(picture: $0) }
      state.pane = .activity(workingActivity)
      return state
    }()

    /// Activity with TextEdit opened by the person, its newest eight steps and Show 1 more, and Notes folded.
    @MainActor public static let viewerActivityOpened: ViewerState = {
      var state = viewerActivity
      state.pane = .activity(openedActivity)
      return state
    }()

    /// Activity under Problems: only the click in TextEdit that failed, its moment held open, and what came next.
    @MainActor public static let viewerActivityProblems: ViewerState = {
      var state = viewerActivity
      state.pane = .activity(problemsActivity)
      return state
    }()

    /// Activity from the days before: a quiet heading over yesterday's Safari and the day before's BuildTool, a copy
    /// whose icon is its tile.
    @MainActor public static let viewerActivityDays: ViewerState = {
      var state = viewerIdle
      state.sidebar.library = viewerLibrary(.activity, counts: [.activity: 0, .screenshots: 6])
      state.mini = SampleDesktop.picture
        .map { MiniScreenState(picture: $0) }
      state.pane = .activity(daysActivity)
      return state
    }()

    /// A morning of an agent moving between apps every few minutes: the newest moment open, the rest folded.
    @MainActor public static let viewerActivityMany: ViewerState = {
      var state = viewerIdle
      state.sidebar.library = viewerLibrary(.activity, counts: [.activity: 20, .screenshots: 6])
      state.mini = SampleDesktop.picture
        .map { MiniScreenState(picture: $0) }
      state.pane = .activity(manyActivity)
      return state
    }()

    /// Activity before anything was done in the helper.
    @MainActor public static let viewerActivityEmpty: ViewerState = {
      var state = viewerIdle
      state.footer.latest = viewerStateLine("Idle", "Ready for your agents")
      state.sidebar.library = viewerLibrary(.activity, counts: [.activity: 0])
      state.mini = SampleDesktop.picture
        .map { MiniScreenState(picture: $0) }
      state.pane = .activity(
        ViewerActivityState(
          meta: "",
          filters: activityFilters(),
          items: [],
          empty: EmptyMessageState(
            headline: "Nothing has happened yet",
            message: "What agents and you do in the helper shows up here."
          ),
          showsFilters: false
        )
      )
      return state
    }()

    /// Activity while the picture is paused: the screen in the corner fades, and the list is as it was.
    @MainActor public static let viewerActivityPaused: ViewerState = {
      var state = viewerPaused
      state.sidebar.library = viewerLibrary(.activity, counts: [.activity: 19, .screenshots: 6])
      state.mini = SampleDesktop.picture
        .map { MiniScreenState(picture: $0, isPaused: true) }
      state.pane = viewerActivity.pane
      return state
    }()
  }
#endif
