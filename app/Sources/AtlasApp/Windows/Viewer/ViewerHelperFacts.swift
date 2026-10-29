import AtlasCommands
import Foundation

/// What the helper line's tooltip lists that the helper's status does not carry, read once the window first shows
/// and by Check Again.
struct ViewerHelperFacts {
  /// The agents connected to Atlas, as last read; nil before they are read, or when they could not be.
  var agents: [AgentID]?
  /// When the helper's account logged in, as last read; nil when unknown.
  var loggedInAt: Date?
}
