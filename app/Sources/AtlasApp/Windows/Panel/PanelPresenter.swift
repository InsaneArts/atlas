import AtlasStatus
import AtlasUI

/// Builds the panel's state from what its controller holds. Pure: every choice the panel shows is made here, from
/// the snapshot, the status table (`Mode+StatusSummary`) and AtlasStatus's rules (``Mode`` and ``StatusReading``). Its
/// buttons and ⋯ menu are in `PanelPresenter+Controls`.
enum PanelPresenter {
  /// The panel as it should look now.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: Everything the panel draws.
  static func state(_ snapshot: PanelSnapshot) -> PanelState {
    let mode = mode(snapshot)
    let reading = StatusReading(snapshot.status)
    return PanelState(
      status: mode.panelSummary(reading),
      notice: snapshot.session.notice,
      picture: picture(snapshot, mode: mode, reading: reading),
      recent: hidesRecent(mode) ? [] : reading.recentEntries,
      controls: controls(snapshot, mode: mode)
    )
  }

  /// The helper's mode as the panel and the menu bar glyph show it: connecting before the first answer, You have
  /// control while Atlas's main window holds it, In use elsewhere while another window does, else the service's
  /// state.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The mode.
  static func mode(_ snapshot: PanelSnapshot) -> Mode {
    if snapshot.connecting { return .connecting }
    return Mode.seen(by: snapshot.status.overview, holdingControl: snapshot.holdingControl)
  }

  /// What fills the picture's frame: the empty stage saying why while the helper is logged out or not ready; else the
  /// screen, inside the rim while in control, dimmed while it needs the person, faded while paused, with the newest
  /// action over it while an agent works and the picture is live.
  ///
  /// - Parameters:
  ///   - snapshot: What the controller holds.
  ///   - mode: The helper's mode.
  ///   - reading: The status, read.
  /// - Returns: The picture's state.
  static func picture(_ snapshot: PanelSnapshot, mode: Mode, reading: StatusReading) -> PanelPictureState {
    switch mode.kind {
    case .loggedOut: return .empty(PanelWords.loggedOutPicture)
    // Recover is offered only where it fixes something (``helperMenu``), and the stage says so only then.
    case .notReady:
      return .empty(
        snapshot.status.overview.recoveryHelps ? PanelWords.notReadyPicture : PanelWords.notReadyWaitPicture
      )
    case .idle, .working, .control, .needsYou, .stopped, .connecting, .inUseElsewhere: break
    }
    let paused = snapshot.session.paused
    let look: PictureLook =
      switch mode.kind {
      case .control: .inControl
      case .needsYou: .dimmed
      default: paused ? .paused : .live
      }
    let caption = mode.kind == .working && !paused ? reading.captionEntries.last : nil
    return .screen(picture: snapshot.picture, look: look, caption: caption)
  }

  /// Whether Recent hides: while the helper is logged out or not ready, as its picture does.
  ///
  /// - Parameter mode: The helper's mode.
  /// - Returns: Whether Recent hides.
  static func hidesRecent(_ mode: Mode) -> Bool {
    mode.kind == .loggedOut || mode.kind == .notReady
  }
}
