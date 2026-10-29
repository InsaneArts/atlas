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

  /// The newest actions, newest first, as the panel's Recent lists them: at most three, each with its age in the
  /// short words a row's right edge holds.
  var recentEntries: [ActivityEntry] {
    activity(limit: 3)
      .enumerated()
      .map { index, item in
        var entry = Self.entry(item, isNewest: index == 0)
        entry.when = item.age.map(Words.shortAgo) ?? ""
        return entry
      }
  }

  /// Each readiness check, as the Ready card lists it.
  var readinessItems: [ReadinessItem] {
    statusRows.map { ReadinessItem(name: $0.subject.word, text: $0.detail.word, state: $0.state.readiness) }
  }

  /// One action in AtlasUI's terms: what was done, by whom, and its mark: a warning sign for a failure, the not-sent
  /// mark for an action refused before anything was sent, which is no problem, and a quiet dot otherwise.
  ///
  /// - Parameters:
  ///   - item: The action.
  ///   - isNewest: Whether it is the newest in its list.
  /// - Returns: The entry.
  private static func entry(_ item: ActivityItem, isNewest: Bool) -> ActivityEntry {
    let first = item.text.prefix(1)
    let line = item.actor == .agent ? first.uppercased() + item.text.dropFirst() : item.line
    let mark: ActivityMarkKind =
      switch item.row.outcome {
      case .running, .worked: .quiet
      case .notSent: .notSent
      case .failed: .failed
      }
    return ActivityEntry(id: item.id, line: line, when: item.when, mark: mark, isNewest: isNewest)
  }
}
