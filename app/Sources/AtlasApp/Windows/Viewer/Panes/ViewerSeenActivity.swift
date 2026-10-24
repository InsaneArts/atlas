import AtlasStatus

/// What of the helper's activity the person has seen (v3 V2): whatever is newer than all of it is unseen, and
/// Activity's tab carries a dot for it until it is chosen, orange once an unseen action failed.
struct ViewerSeenActivity: Equatable {
  /// The actions seen, by id: those the Activity tab showed, or those the first reading held, so what the helper did
  /// before the window opened is never new. Empty when there were none, so every action since is unseen.
  var ids: Set<String>
  /// Whether an action not yet seen failed. Kept from the reading that held it until Activity shows, so the dot stays
  /// orange even once newer actions push that one out of the few the status lists.
  var failed = false

  /// The actions in `activity`, newest first, newer than every one seen.
  ///
  /// - Parameter activity: The status's recent actions, newest first.
  /// - Returns: The unseen ones, newest first.
  func unseen(in activity: [ActivityItem]) -> ArraySlice<ActivityItem> {
    activity.prefix { !ids.contains($0.id) }
  }
}
