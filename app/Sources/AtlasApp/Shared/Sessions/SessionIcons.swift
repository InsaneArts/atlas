import AtlasService
import AtlasStatus
import AtlasUI

/// The icon of the session an action came from, for the lines the panel and the main window list.
enum SessionIcons {
  /// The icon of the session an action came from: Terminal's `>_` tile for an action no host named, else its listed
  /// session's host. A session the status does not list gets none, since nothing says which host runs it.
  ///
  /// - Parameters:
  ///   - key: The action's session key; empty for Terminal.
  ///   - reading: The status, read.
  /// - Returns: The icon, or nil for a session the status does not list.
  static func icon(of key: String, in reading: StatusReading) -> RowIcon? {
    guard !key.isEmpty else { return SessionHost.terminal.icon }
    return reading.session(key)
      .map(\.host.icon)
  }
}
