import AtlasKit

extension ActivityItem.Actor {
  /// The actor in words, as a row starts.
  var word: String {
    switch self {
    case .you: "You"
    case .agent: "Agent"
    }
  }
}
