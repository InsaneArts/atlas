import AtlasService
import AtlasUI
import Foundation

/// What the Activity tab last read, and what the person asked of it: the search and the filter.
struct ViewerActivityFacts {
  /// The log, newest first; nil before the first reading.
  var log: ActivityLog?
  /// When the log was read, which each row's age counts back from.
  var readAt: Date?
  /// Why the last reading failed, while it has; nil once one worked.
  var failure: String?
  /// The search, as typed.
  var query = ""
  /// The filter chosen.
  var filter = ViewerActivityFilter.all
  /// Counts Find's asks for the search field's focus.
  var findRequests = 0
}
