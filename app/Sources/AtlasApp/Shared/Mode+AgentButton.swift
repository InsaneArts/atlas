import AtlasKit
import AtlasUI

extension Mode {
  /// The button for agents, as the main window's top row and the menu bar panel both offer it: Stop while an agent
  /// acts, or acted a moment ago (``ViewerStatus/Overview/offersStop``), Resume once the person stopped agents,
  /// waiting while `isBusy`; neither while the person holds control, when no agent acts.
  ///
  /// - Parameters:
  ///   - overview: The service's overview.
  ///   - holdingControl: Whether the window asking, or for the panel Atlas's main window, holds control.
  ///   - isBusy: Whether something else is asked of the service now, so Resume waits; Stop never does.
  /// - Returns: The button, or nil for none.
  func agentButton(_ overview: ViewerStatus.Overview, holdingControl: Bool, isBusy: Bool) -> ViewerAgentButton? {
    if holdingControl || kind == .control { return nil }
    if overview.offersResume { return .resume(isEnabled: !isBusy) }
    return overview.offersStop ? .stop : nil
  }
}
