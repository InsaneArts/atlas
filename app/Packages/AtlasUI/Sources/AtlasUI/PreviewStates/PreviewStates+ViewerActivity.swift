#if DEBUG
  import Foundation

  extension PreviewStates {
    /// The Activity pane's filters, one of them chosen, Problems and Pictures with their counts.
    ///
    /// - Parameters:
    ///   - selected: The filter chosen.
    ///   - problems: How many problems; none drawn for zero.
    ///   - pictures: How many pictures; none drawn for zero.
    /// - Returns: The filters.
    static func activityFilters(
      _ selected: ViewerActivityFilter = .all,
      problems: Int = 0,
      pictures: Int = 0
    ) -> CapsuleTabsState<ViewerActivityFilter> {
      CapsuleTabsState(
        tabs: [
          CapsuleTab(id: .all, title: "All"),
          CapsuleTab(id: .problems, title: "Problems", count: problems > 0 ? problems : nil),
          CapsuleTab(id: .pictures, title: "Pictures", count: pictures > 0 ? pictures : nil),
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
    /// - Returns: What leads its run.
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

    /// The picture taken before a step in a sample, at the shape a picture no screenshot list names takes until it is
    /// read, its thumbnail read.
    ///
    /// - Parameter name: The picture's file name.
    /// - Returns: The picture.
    @MainActor static func samplePicture(_ name: String) -> ViewerActivityShot {
      ViewerActivityShot(name: name, thumbnail: shotPicture(name))
    }

    /// What the Activity sample's agent typed into Opie's Name field.
    public static let sampleTyped = "Ring settings"

    /// Where the Activity sample's typing and key press went in Opie: its Name field, a little left of the middle.
    public static let sampleSpot = CGPoint(x: 0.35, y: 0.375)

    /// The Activity sample's typing, marked on the picture before it: a ring on the field, the text in a bubble.
    @MainActor static var sampleTypingPicture: ViewerStepPicture {
      ViewerStepPicture(
        shot: samplePicture(beforeNames[0]),
        mark: ViewerActionMark(shape: .typing(text: sampleTyped, detail: "Typed · 13 characters"), spot: sampleSpot)
      )
    }

    /// The Activity sample's ⌘S, marked on the picture before it: a soft ring on the field, the keys over it.
    @MainActor static var sampleKeysPicture: ViewerStepPicture {
      ViewerStepPicture(
        shot: samplePicture(beforeNames[1]),
        mark: ViewerActionMark(shape: .keys([["⌘", "S"]]), spot: sampleSpot)
      )
    }

    /// 16:9: the samples' captures' shape, 2560 by 1440.
    static let shotAspect: CGFloat = 2560.0 / 1440.0

    /// Activity while an agent works, with the live screen in the corner: Claude Code's session today with its four
    /// runs folded to their problems and pictures, and yesterday's Terminal folded.
    @MainActor public static let viewerActivity: ViewerState = {
      var state = viewerWorking
      state.sidebar.sessions.chosen = .all
      state.mini = SampleDesktop.picture
        .map { MiniScreenState(picture: $0) }
      state.pane = .activity(workingActivity)
      return state
    }()

    /// Activity with the Chrome run opened by the person: its steps oldest first, its screenshot in place.
    @MainActor public static let viewerActivityOpened: ViewerState = {
      var state = viewerActivity
      state.pane = .activity(openedActivity)
      return state
    }()

    /// Activity under Problems: only the Chrome run, open to the press that failed.
    @MainActor public static let viewerActivityProblems: ViewerState = {
      var state = viewerActivity
      state.pane = .activity(problemsActivity)
      return state
    }()

    /// Activity from the days before: yesterday's Terminal, then Wednesday's sessions and the row for the rest.
    @MainActor public static let viewerActivityDays: ViewerState = {
      var state = viewerIdle
      state.sidebar.sessions.chosen = .all
      state.mini = SampleDesktop.picture
        .map { MiniScreenState(picture: $0) }
      state.pane = .activity(daysActivity)
      return state
    }()

    /// A long list: today's live session, then a day of many short sessions and the row for the rest.
    @MainActor public static let viewerActivityMany: ViewerState = {
      var state = viewerIdle
      state.sidebar.sessions.chosen = .all
      state.mini = SampleDesktop.picture
        .map { MiniScreenState(picture: $0) }
      state.pane = .activity(manyActivity)
      return state
    }()

    /// Activity before anything was done in the helper.
    @MainActor public static let viewerActivityEmpty: ViewerState = {
      var state = viewerIdle
      state.sidebar.sessions.chosen = .all
      state.footer.latest = viewerStateLine("Idle", "Ready for your agents")
      state.mini = SampleDesktop.picture
        .map { MiniScreenState(picture: $0) }
      state.pane = .activity(
        ViewerActivityState(
          meta: "",
          filters: activityFilters(),
          days: [],
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
      state.sidebar.sessions.chosen = .all
      state.mini = SampleDesktop.picture
        .map { MiniScreenState(picture: $0, isPaused: true) }
      state.pane = viewerActivity.pane
      return state
    }()

    /// Activity with the live TextEdit run's pictures opened in place, and Show fewer.
    @MainActor public static let viewerActivityShots: ViewerState = {
      var state = viewerActivity
      state.pane = .activity(shotsActivity)
      return state
    }()

    /// A screenshot of the live TextEdit run open in the picture sheet over Activity, its tile marked current.
    @MainActor public static let viewerPictureSheet: ViewerState = {
      var activity = shotsActivity
      activity.days[0].sessions[0].runs[0].grid?.tiles[0].isCurrent = true
      activity.sheet = pictureSheet
      var state = viewerActivity
      state.pane = .activity(activity)
      return state
    }()

    /// The screen just before Return was pressed in Chrome, open in the picture sheet over Claude Code's runs.
    @MainActor public static let viewerStepPicture: ViewerState = {
      var activity = oneSessionActivity
      activity.sheet = stepPictureSheet
      var state = viewerActivityOneSession
      state.pane = .activity(activity)
      return state
    }()
  }
#endif
