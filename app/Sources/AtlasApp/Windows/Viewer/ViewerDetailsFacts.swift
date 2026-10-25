import AtlasCommands
import Foundation

/// Details as this window holds it: whether the person asked for it, and what it lists that the helper's status does
/// not carry, read when it opens and by Check Again.
struct ViewerDetailsFacts {
  /// Whether the person asked for Details.
  var shown = false
  /// The agents connected to Atlas, as last read for Details; nil before they are read, or when they could not be.
  var agents: [AgentID]?
  /// When the helper's account logged in, as last read for Details; nil when unknown.
  var loggedInAt: Date?
}
