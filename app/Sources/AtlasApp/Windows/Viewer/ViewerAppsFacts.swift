import AtlasKit
import AtlasUI

/// What the Apps tab last read, its icons decoded, the search, and the window shown on the screen.
struct ViewerAppsFacts {
  /// The apps, front to back; nil before the first reading.
  var apps: [HelperApp]?
  /// The apps' icons, decoded once each, by what they are known by (``HelperApp/iconID``).
  var icons: [String: IconImage] = [:]
  /// Why the last reading failed, while it has; nil once one worked.
  var failure: String?
  /// The search, as typed.
  var query = ""
  /// The window outlined on the screen; nil shows none.
  var shown: ViewerShownWindow?
}
