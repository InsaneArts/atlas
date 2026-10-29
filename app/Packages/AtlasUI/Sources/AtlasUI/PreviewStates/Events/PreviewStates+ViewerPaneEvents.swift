#if DEBUG
  import CoreGraphics

  extension PreviewStates {
    /// The Screenshots sample's captures, newest first, by their place in ``shotNames`` and their time.
    private static let shotTimes = [
      (0, "10:31:45"), (1, "10:31:44"), (2, "10:31:20"), (3, "10:31:14"), (4, "10:12:03"), (5, "09:58:41"),
    ]

    /// A tab's sample after a filter, a moment opened or folded, or a screenshot picked or moved through, so the
    /// Interactive previews answer them roughly as the app does: the samples' own steps and screenshots, narrowed and
    /// selected.
    ///
    /// - Parameters:
    ///   - state: The sample shown.
    ///   - event: What was done.
    /// - Returns: The sample to show next.
    @MainActor static func viewerPane(_ state: ViewerState, applying event: ViewerEvent) -> ViewerState {
      var next = state
      switch (state.pane, event) {
      case (.activity(let activity), .filterActivity(let filter)):
        next.pane = .activity(activitySample(activity, query: state.sidebar.search.query, filter: filter))
      case (.activity(let activity), .toggleMoment(let id, let opens)):
        next.pane = .activity(activitySample(activity, moment: id, opens: opens))
      case (.activity(let activity), .showMomentSteps(let id)):
        next.pane = .activity(activitySample(activity, moment: id, opens: true, every: true))
      case (.screenshots(let shots), _):
        next.pane = .screenshots(shotsSample(shots, applying: event))
      case (_, .openShot(let name)):
        next = viewerShots
        next.pane = .screenshots(shotsSample(selected: name, large: name))
      default:
        return state
      }
      return next
    }

    /// The Activity or Apps sample narrowed by the sidebar's search, as the list shown follows it.
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
      case .apps(var apps):
        apps.groups = sampleAppsPane.groups
          .map { group in
            var kept = group
            kept.cards = group.cards.filter { query.isEmpty || $0.name.localizedCaseInsensitiveContains(query) }
            return kept
          }
          .filter { !$0.cards.isEmpty }
        next.pane = .apps(apps)
      case .screen, .screenshots:
        break
      }
      return next
    }

    /// The Activity sample narrowed by a search and a filter: each open moment's steps the filter and the search keep,
    /// and the moments left with any.
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
      var kept: [ViewerActivityItem] = []
      for item in activity.items {
        switch item {
        case .step(let step):
          let passes =
            switch filter {
            case .all: true
            case .problems: step.mark == .failed
            case .screenshots: step.kind == .screenshot
            case .typing: step.kind == .typing || step.kind == .keys
            }
          if passes, query.isEmpty || step.words.text.localizedCaseInsensitiveContains(query) {
            kept.append(item)
          }
        case .moment, .day, .more:
          kept.append(item)
        }
      }
      next.items = withoutEmptyMoments(kept, keepFolded: filter == .all && query.isEmpty)
      next.empty =
        next.items.isEmpty
        ? EmptyMessageState(headline: "No actions match", message: "Choose All, or search for other words.") : nil
      return next
    }

    /// The Activity sample with a moment opened or folded: its steps from whichever sample lists them, or none.
    ///
    /// - Parameters:
    ///   - activity: The pane shown.
    ///   - id: The moment's identity.
    ///   - opens: Whether it opens.
    ///   - every: Whether every step it holds shows, as after Show N more.
    /// - Returns: The pane.
    @MainActor private static func activitySample(
      _ activity: ViewerActivityState,
      moment id: String,
      opens: Bool,
      every: Bool = false
    ) -> ViewerActivityState {
      var next = activity
      var items: [ViewerActivityItem] = []
      var inMoment = false
      for item in activity.items {
        if case .moment(var moment) = item {
          inMoment = moment.id == id
          if inMoment {
            moment.isOpen = opens
            items.append(.moment(moment))
            if opens { items += sampleSteps(of: id, every: every) }
            continue
          }
        } else if inMoment {
          continue
        }
        items.append(item)
      }
      next.items = items
      return next
    }

    /// Every sample's rows, for the steps a moment opened in a preview shows.
    @MainActor private static var sampleRows: [ViewerActivityItem] {
      [openedActivity, workingActivity, bySessionActivity, oneSessionActivity, manyActivity, daysActivity]
        .flatMap(\.items)
    }

    /// The steps a sample lists under a moment, and Show N more unless every step shows.
    ///
    /// - Parameters:
    ///   - id: The moment's identity.
    ///   - every: Whether every step shows.
    /// - Returns: The rows.
    @MainActor private static func sampleSteps(of id: String, every: Bool) -> [ViewerActivityItem] {
      var steps: [ViewerActivityItem] = []
      var inMoment = false
      for item in sampleRows {
        if case .moment(let moment) = item {
          if inMoment, !steps.isEmpty { break }
          inMoment = moment.id == id
          continue
        }
        guard inMoment else { continue }
        if case .more = item, every { continue }
        steps.append(item)
      }
      return steps
    }

    /// The rows less the moments a filter or a search left with no steps.
    ///
    /// - Parameters:
    ///   - items: The rows.
    ///   - keepFolded: Whether folded moments stay, as under All with no search.
    /// - Returns: The rows.
    private static func withoutEmptyMoments(_ items: [ViewerActivityItem], keepFolded: Bool) -> [ViewerActivityItem] {
      items.enumerated()
        .filter { index, item in
          guard case .moment = item, !keepFolded else { return true }
          let next = items.index(after: index)
          guard next < items.endIndex, case .step = items[next] else { return false }
          return true
        }
        .map(\.element)
    }

    /// The Screenshots sample after one of its events.
    ///
    /// - Parameters:
    ///   - shots: The pane shown.
    ///   - event: What was done.
    /// - Returns: The pane.
    @MainActor private static func shotsSample(_ shots: ViewerShotsState, applying event: ViewerEvent)
      -> ViewerShotsState
    {
      let tiles = shots.days.flatMap(\.shots)
      let selected = tiles.first(where: \.isSelected)?.id
      let large = shots.large?.id
      switch event {
      case .selectShot(let name): return shotsSample(selected: name, large: large)
      case .openShot(let name): return shotsSample(selected: name, large: name)
      case .toggleShot: return shotsSample(selected: selected, large: large == nil ? selected : nil)
      case .closeShot: return shotsSample(selected: selected, large: nil)
      case .previousShot, .nextShot:
        let ids = tiles.map(\.id)
        let from = ids.firstIndex(of: large ?? selected ?? "") ?? 0
        let to = min(max(from + (event == .nextShot ? 1 : -1), 0), max(ids.count - 1, 0))
        guard ids.indices.contains(to) else { return shots }
        return shotsSample(selected: ids[to], large: large == nil ? nil : ids[to])
      default: return shots
      }
    }

    /// The Screenshots sample with one capture selected and one perhaps large.
    ///
    /// - Parameters:
    ///   - selected: The capture selected; the first when it is not one of them.
    ///   - large: The capture shown large.
    /// - Returns: The pane.
    @MainActor private static func shotsSample(selected: String?, large: String?) -> ViewerShotsState {
      let kept = shotTimes
      let ids = kept.map { shotNames[$0.0] }
      let picked = ids.contains(selected ?? "") ? selected : ids.first
      let tiles = kept.map { shotTile($0.0, $0.1, isSelected: shotNames[$0.0] == picked) }
      let days = [
        ViewerShotDay(id: "hour", title: "Now", shots: tiles.filter { $0.time.hasPrefix("10:31") }),
        ViewerShotDay(id: "morning", title: "Earlier today", shots: tiles.filter { !$0.time.hasPrefix("10:31") }),
      ]
      .filter { !$0.shots.isEmpty }
      let largeShot = large.flatMap { name in tiles.first { $0.id == name } }
        .map { tile in
          ViewerLargeShot(
            id: tile.id,
            picture: tile.thumbnail,
            aspect: ViewerLayout.placeholderAspect,
            caption: "\(tile.time) · 2560 × 1440",
            canGoBack: tile.id != ids.first,
            canGoForward: tile.id != ids.last,
            canShowInFinder: true
          )
        }
      return ViewerShotsState(meta: "6 today", days: days, large: largeShot)
    }
  }
#endif
