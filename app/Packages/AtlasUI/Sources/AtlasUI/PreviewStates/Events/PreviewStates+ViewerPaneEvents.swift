#if DEBUG
  import CoreGraphics

  extension PreviewStates {
    /// The Screenshots sample's captures, newest first, by their place in ``shotNames`` and their time.
    private static let shotTimes = [
      (0, "10:31:45"), (1, "10:31:44"), (2, "10:31:20"), (3, "10:31:14"), (4, "10:12:03"), (5, "09:58:41"),
    ]

    /// A tab's sample after a search, a filter, a screenshot picked or moved through, so the Interactive previews
    /// answer them roughly as the app does: the samples' own lines, apps and screenshots, narrowed and selected.
    ///
    /// - Parameters:
    ///   - state: The sample shown.
    ///   - event: What was done.
    /// - Returns: The sample to show next.
    @MainActor static func viewerPane(_ state: ViewerState, applying event: ViewerEvent) -> ViewerState {
      var next = state
      switch (state.pane, event) {
      case (.activity(let activity), .searchActivity(let query)):
        next.pane = .activity(activitySample(activity, query: query, filter: activity.filters.selected))
      case (.activity(let activity), .filterActivity(let filter)):
        next.pane = .activity(activitySample(activity, query: activity.query, filter: filter))
      case (.apps(var apps), .searchApps(let query)):
        apps.query = query
        apps.apps = sampleApps.filter { query.isEmpty || $0.name.localizedCaseInsensitiveContains(query) }
        next.pane = .apps(apps)
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

    /// The Activity sample narrowed by a search and a filter.
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
      next.query = query
      next.filters.selected = filter
      next.groups = activityGroups.compactMap { group in
        var kept = group
        kept.lines = group.lines.filter { line in
          let shown =
            switch filter {
            case .all: true
            case .agent: line.lead != .person
            case .you: line.lead == .person
            case .problems: line.lead == .failed
            }
          return shown && (query.isEmpty || line.text.localizedCaseInsensitiveContains(query))
        }
        return kept.lines.isEmpty ? nil : kept
      }
      next.empty =
        next.groups.isEmpty
        ? EmptyMessageState(headline: "No actions match", message: "Choose All, or search for other words.") : nil
      return next
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
