/// The main menu's words that no window owns: the menus' own titles, the items every Mac app has, and the helper's
/// commands as the Helper menu names them.
enum MenuWords {
  /// The app's menu.
  static let app = "Atlas"
  /// About Atlas.
  static let about = "About Atlas"
  /// Settings….
  static let settings = "Settings…"
  /// Check for Updates…, which opens Settings on About.
  static let checkForUpdates = "Check for Updates…"
  /// Services.
  static let services = "Services"
  /// Hide Atlas.
  static let hide = "Hide Atlas"
  /// Hide Others.
  static let hideOthers = "Hide Others"
  /// Show All.
  static let showAll = "Show All"
  /// Quit Atlas.
  static let quit = "Quit Atlas"
  /// The File menu.
  static let file = "File"
  /// Open Atlas: the main window.
  static let openAtlas = "Open Atlas"
  /// Close.
  static let close = "Close"
  /// The Edit menu.
  static let edit = "Edit"
  /// Undo.
  static let undo = "Undo"
  /// Redo.
  static let redo = "Redo"
  /// Cut.
  static let cut = "Cut"
  /// Copy.
  static let copy = "Copy"
  /// Paste.
  static let paste = "Paste"
  /// Select All.
  static let selectAll = "Select All"
  /// Find…: the shown tab's search.
  static let find = "Find…"
  /// The View menu.
  static let view = "View"
  /// The Helper menu.
  static let helper = "Helper"
  /// The Helper menu's Recover… with Option held: restarting the helper's service, for when Recover cannot help.
  static let restartHelper = "Restart Helper…"
  /// The Window menu.
  static let window = "Window"
  /// Minimize.
  static let minimize = "Minimize"
  /// Zoom.
  static let zoom = "Zoom"
  /// Bring All to Front.
  static let bringAllToFront = "Bring All to Front"
  /// The Help menu.
  static let help = "Help"
  /// Getting Started….
  static let gettingStarted = "Getting Started…"
  /// Copy Diagnostics: what `atlas doctor` says, on the clipboard.
  static let copyDiagnostics = "Copy Diagnostics"
  /// Once the diagnostics are on the clipboard.
  static let diagnosticsCopied = "Diagnostics copied."
  /// The diagnostics could not be read.
  static let diagnosticsFailed = "Atlas couldn't read its diagnostics."

  /// The Helper menu's switch, naming the helper once a window knows it.
  ///
  /// - Parameter helper: The helper as people know it; nil before a window knows it.
  /// - Returns: "Switch to “Atlas”…", or "Switch to the Helper…".
  static func switchTo(_ helper: String?) -> String {
    helper.map { "Switch to “\($0)”…" } ?? HelperWords.switchItem
  }

  /// The Helper menu's log-out, naming the helper once a window knows it.
  ///
  /// - Parameter helper: The helper as people know it; nil before a window knows it.
  /// - Returns: "Log Out “Atlas”…", or "Log Out the Helper…".
  static func logOut(_ helper: String?) -> String {
    helper.map { "Log Out “\($0)”…" } ?? HelperWords.logOutItem
  }
}
