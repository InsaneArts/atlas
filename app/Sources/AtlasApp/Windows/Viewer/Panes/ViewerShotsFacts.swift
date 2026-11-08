import AtlasService
import Foundation

/// The screenshots' list as last read with the activity: each one's name, time and size, and the folder they are
/// kept in, which size the outline's pictures and find their files.
struct ViewerShotsFacts {
  /// The screenshots, newest first; nil before the first reading.
  var shots: [Screenshot]?
  /// The folder they are kept in, when the service names one.
  var folder: URL?
}
