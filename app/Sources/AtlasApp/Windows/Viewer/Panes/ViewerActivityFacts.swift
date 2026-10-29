import AtlasService
import AtlasUI
import Foundation

/// What the Activity tab last read, and what the person asked of it: the filter, the moments they opened or folded,
/// and those they asked to list every step of, each kept for the window's life. The sidebar's search narrows it.
struct ViewerActivityFacts {
  /// The log, newest first; nil before the first reading.
  var log: ActivityLog?
  /// The preceding reading's moments, whose identities survive overlapping log windows.
  var moments: [ActivityMoment] = []
  /// When the log was read, which each row's age counts back from.
  var readAt: Date?
  /// Why the last reading failed, while it has; nil once one worked.
  var failure: String?
  /// The filter chosen.
  var filter = ViewerActivityFilter.all
  /// The person's own choice for a moment, by its id: true to open it, false to fold it. A choice, not a flip of the
  /// default, so a moment the person opened stays open as newer actions make it the newest.
  var choices: [String: Bool] = [:]
  /// The moments the person asked to list every step of, by their ids.
  var expanded: Set<String> = []
}
