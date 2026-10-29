import AtlasService
import AtlasUI

/// Whose work the window shows, as the rules that pick rows read it.
extension ViewerSessionChoice {
  /// The chosen session's key, empty for Terminal; nil while every session's work shows.
  var key: String? {
    switch self {
    case .all: nil
    case .one(let key): key
    }
  }
}
