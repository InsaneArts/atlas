import AtlasKit
import AtlasUI

extension Mode {
  /// The mode in AtlasUI's own terms, for a status badge.
  var summary: StatusSummary {
    StatusSummary(label: label, tone: tone.statusTone)
  }
}
