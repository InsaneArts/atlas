import AtlasService
import AtlasStatus
import AtlasUI

extension Mode {
  /// The button for agents, as the main window's top row and the menu bar panel both offer it: Stop while an agent
  /// acts, or acted a moment ago, Resume once the person stopped agents, waiting while `isBusy`. While the person holds
  /// control, here or elsewhere, Stop follows ``ViewerStatus/Overview/offersStop(holdingControl:)`` and Resume is not
  /// offered.
  ///
  /// - Parameters:
  ///   - overview: The service's overview.
  ///   - holdingControl: Whether the window asking, or for the panel Atlas's main window, holds control.
  ///   - isBusy: Whether something else is asked of the service now, so Resume waits; Stop never does.
  /// - Returns: The button, or nil for none.
  func agentButton(_ overview: ViewerStatus.Overview, holdingControl: Bool, isBusy: Bool) -> ViewerAgentButton? {
    let controlling = holdingControl || kind == .control
    if !controlling, overview.offersResume { return .resume(isEnabled: !isBusy) }
    return overview.offersStop(holdingControl: controlling) ? .stop : nil
  }
}
