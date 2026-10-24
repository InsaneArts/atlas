import AtlasService
import AtlasUI

/// What the panel asks of the rest of the app, as closures ``WindowCoordinator`` gives it at `init`: the panel never
/// touches another window itself.
struct PanelLinks {
  /// Opens the main window, on `tab` or, for nil, on the tab it shows.
  var openViewer: @MainActor (ViewerTab?) -> Void = { _ in }
  /// Opens the Settings window on Helper, which says what the helper needs.
  var openSettings: @MainActor () -> Void = {}
  /// Asks before one of the helper's commands runs: in the main window when it is open, else in the alert window.
  var showAlert: @MainActor (HelperAlert) -> Void = { _ in }
  /// Whether Atlas's main window holds control now.
  var holdingControl: @MainActor () -> Bool = { false }
  /// Ends the control Atlas's main window holds.
  var giveBack: @MainActor () -> Void = {}
  /// Quits Atlas.
  var quit: @MainActor () -> Void = {}
  /// Hands on each status reading that ended: the service's answer, or nil when the reading failed. The panel reads
  /// the status all the time, so what Atlas shows outside its windows follows it.
  var read: @MainActor (ViewerStatus?) -> Void = { _ in }
}
