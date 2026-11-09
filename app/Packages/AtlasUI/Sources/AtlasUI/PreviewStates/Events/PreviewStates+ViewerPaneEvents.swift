#if DEBUG
  import CoreGraphics

  extension PreviewStates {
    /// A tab's sample after a filter, a session or run opened or folded, pictures opened in place, or a picture opened,
    /// so the Interactive previews answer them roughly as the app does: the samples' own runs and steps, narrowed.
    ///
    /// - Parameters:
    ///   - state: The sample shown.
    ///   - event: What was done.
    /// - Returns: The sample to show next.
    @MainActor static func viewerPane(_ state: ViewerState, applying event: ViewerEvent) -> ViewerState {
      guard case .activity(let activity) = state.pane else { return state }
      var next = state
      switch event {
      case .filterActivity(let filter):
        next.pane = .activity(activitySample(activity, query: state.sidebar.search.query, filter: filter))
      case .toggleSession(let id, let opens):
        next.pane = .activity(outlineSample(activity, session: { if $0.id == id { $0.isOpen = opens } }))
      case .toggleRun(let id, let opens):
        next.pane = .activity(outlineSample(activity, run: { if $0.id == id { opened(&$0, opens) } }))
      case .showRunPictures(let id), .showAllRunPictures(let id):
        let all = event == .showAllRunPictures(id)
        next.pane = .activity(outlineSample(activity, run: { if $0.id == id { pictures(&$0, all: all) } }))
      case .hideRunPictures(let id):
        next.pane = .activity(outlineSample(activity, run: { if $0.id == id { $0.grid = nil } }))
      case .openAction, .openShot, .closePicture, .pictureSide, .openReplay, .replayFrame, .previousFrame, .nextFrame,
        .togglePlay, .closeReplay:
        next.pane = .activity(actionSample(activity, applying: event))
      default:
        return state
      }
      return next
    }

    /// The Activity sample narrowed by the sidebar's search, as the list shown follows it.
    ///
    /// - Parameters:
    ///   - state: The sample shown.
    ///   - query: The search.
    /// - Returns: The sample, its list narrowed; any other place as it was.
    @MainActor static func viewerPane(_ state: ViewerState, searching query: String) -> ViewerState {
      var next = state
      switch state.pane {
      case .activity(let activity):
        next.pane = .activity(activitySample(activity, query: query, filter: activity.filters.selected))
      case .screen:
        break
      }
      return next
    }

    /// The sample with every session and run changed as asked.
    ///
    /// - Parameters:
    ///   - activity: The pane shown.
    ///   - session: What happens to each session.
    ///   - run: What happens to each run.
    /// - Returns: The pane.
    @MainActor private static func outlineSample(
      _ activity: ViewerActivityState,
      session: (inout ViewerOutlineSession) -> Void = { _ in },
      run: (inout ViewerOutlineRun) -> Void = { _ in }
    ) -> ViewerActivityState {
      var next = activity
      for day in next.days.indices {
        for place in next.days[day].sessions.indices {
          session(&next.days[day].sessions[place])
          for index in next.days[day].sessions[place].runs.indices {
            run(&next.days[day].sessions[place].runs[index])
          }
        }
      }
      return next
    }

    /// A run opened or folded, its steps from the sample that lists them.
    ///
    /// - Parameters:
    ///   - run: The run.
    ///   - opens: Whether it opens.
    @MainActor private static func opened(_ run: inout ViewerOutlineRun, _ opens: Bool) {
      run.isOpen = opens
      run.canReplay = opens
      run.steps = opens ? (run.id == chromeRun.id ? chromeSteps : run.problemSteps) : []
    }

    /// A run's pictures opened in place: its first eight, or every one.
    ///
    /// - Parameters:
    ///   - run: The run.
    ///   - all: Whether every picture shows.
    @MainActor private static func pictures(_ run: inout ViewerOutlineRun, all: Bool) {
      guard let strip = run.strip else { return }
      let count = all ? min(strip.total, 24) : min(strip.total, ViewerLayout.pictureGridFirst)
      run.grid = ViewerPictureGrid(
        run: run.id,
        tiles: outlineTiles(run.id, count: count),
        more: all ? 0 : strip.total - count
      )
    }

    /// The Activity sample narrowed by a search and a filter: each open run's steps the filter and the search keep,
    /// and the runs left with any.
    ///
    /// - Parameters:
    ///   - activity: The pane shown.
    ///   - query: The search.
    ///   - filter: The filter.
    /// - Returns: The pane.
    @MainActor private static func activitySample(
      _ activity: ViewerActivityState,
      query: String,
      filter: ViewerActivityFilter
    ) -> ViewerActivityState {
      var next = activity
      next.filters.selected = filter
      guard filter != .all || !query.isEmpty else { return next }
      next.days = activity.days.compactMap { day in
        var kept = day
        kept.sessions = day.sessions.compactMap { session in
          var shown = session
          shown.runs = session.runs.compactMap { run in
            var open = run
            opened(&open, true)
            open.steps = open.steps.filter { step in
              let passes =
                switch filter {
                case .all: true
                case .problems: step.mark == .failed
                case .pictures: step.kind == .screenshot
                case .typing: step.kind == .typing || step.kind == .keys
                }
              return passes && (query.isEmpty || step.words.text.localizedCaseInsensitiveContains(query))
            }
            return open.steps.isEmpty ? nil : open
          }
          return shown.runs.isEmpty ? nil : shown
        }
        return kept.sessions.isEmpty ? nil : kept
      }
      next.empty =
        next.days.isEmpty
        ? EmptyMessageState(headline: "No actions match", message: "Choose All, or search for other words.") : nil
      return next
    }
  }
#endif
