import AtlasKit
import AtlasUI

extension StatusReading {
  /// The recent actions, newest first, as the sidebar's rows.
  var recentEntries: [ActivityEntry] {
    let items = activity()
    return items.map { Self.entry($0, isNewest: $0.id == items.first?.id) }
  }

  /// The few latest actions, oldest first, as the captions over the desktop.
  var captionEntries: [ActivityEntry] {
    let items = captions()
    return items.map { Self.entry($0, isNewest: $0.id == items.last?.id) }
  }

  /// Each readiness check, as the Ready card lists it.
  var readinessItems: [ReadinessItem] {
    statusRows.map { ReadinessItem(name: $0.name, text: $0.text, state: $0.state.readiness) }
  }

  /// One action in AtlasUI's terms: its actor's initial, and whose it was as the row colours it, a failure first,
  /// then the person's own, then anyone else's.
  ///
  /// - Parameters:
  ///   - item: The action.
  ///   - isNewest: Whether it is the newest in its list.
  /// - Returns: The entry.
  private static func entry(_ item: ActivityItem, isNewest: Bool) -> ActivityEntry {
    let kind: ActivityKind =
      if !item.ok {
        .failed
      } else if item.actor == .you {
        .yours
      } else {
        .others
      }
    return ActivityEntry(
      id: item.id,
      initial: item.actor.word.initial,
      line: item.line,
      when: item.when,
      kind: kind,
      isNewest: isNewest
    )
  }
}
