import AtlasStatus
import AtlasUI

extension StatusReading {
  /// The few latest actions, oldest first, as the captions over the screen: an agent's by what it did, as the
  /// line beside Working names it, led by its session's icon while sessions show, and the person's as theirs.
  var captionEntries: [ActivityEntry] {
    let items = captions()
    return items.map { item in
      var entry = Self.entry(item, isNewest: item.id == items.last?.id)
      if showsSessions, item.actor == .agent { entry.icon = SessionIcons.icon(of: item.sessionKey, in: self) }
      return entry
    }
  }

  /// The newest of the latest actions, as the main window's footer says it: led by its session's icon whenever an
  /// agent did it and the status lists any session, one or more; nil once none is recent enough for a caption.
  var latestEntry: ActivityEntry? {
    guard let item = captions().last else { return nil }
    var entry = Self.entry(item, isNewest: true)
    if item.actor == .agent, !liveSessions.isEmpty { entry.icon = SessionIcons.icon(of: item.sessionKey, in: self) }
    return entry
  }

  /// The newest actions, newest first, as the panel's Recent lists them: at most three, each with its age in the
  /// short words a row's right edge holds.
  var recentEntries: [ActivityEntry] {
    activity(limit: PanelPresenter.recentRows)
      .enumerated()
      .map { index, item in
        var entry = Self.entry(item, isNewest: index == 0)
        entry.when = item.age.map(Words.shortAgo) ?? ""
        return entry
      }
  }

  /// Each readiness check, as the Ready card lists it: never the native Save and Open check, which is no part of the
  /// helper's readiness.
  var readinessItems: [ReadinessItem] {
    readinessRows.map(Self.item)
  }

  /// The native Save and Open check, in AtlasUI's terms, beside the readiness checks; nil while the service says
  /// nothing of it.
  var nativeDialogsItem: ReadinessItem? {
    nativeDialogsRow.map(Self.item)
  }

  /// One check in AtlasUI's terms: its name, what it found and how it stands.
  ///
  /// - Parameter row: The check.
  /// - Returns: The item.
  private static func item(_ row: StatusRow) -> ReadinessItem {
    ReadinessItem(name: row.subject.word, text: row.detail.word, state: row.state.readiness)
  }

  /// One action in AtlasUI's terms: what was done, by whom, and its mark: a warning sign for a failure, the not-sent
  /// mark for an action refused before anything was sent, which is no problem, and a quiet dot otherwise.
  ///
  /// - Parameters:
  ///   - item: The action.
  ///   - isNewest: Whether it is the newest in its list.
  /// - Returns: The entry.
  private static func entry(_ item: ActivityItem, isNewest: Bool) -> ActivityEntry {
    let line = item.actor == .agent ? Words.leadingCapital(item.text) : item.line
    let mark: ActivityMarkKind =
      switch item.row.outcome {
      case .running, .worked: .quiet
      case .notSent: .notSent
      case .failed: .failed
      }
    return ActivityEntry(id: item.id, line: line, when: item.when, mark: mark, isNewest: isNewest)
  }
}
