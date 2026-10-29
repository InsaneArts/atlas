import AtlasStatus
import AtlasUI
import Foundation

/// Builds the panel's state from what its controller holds. Pure: every choice the panel shows is made here, from
/// the snapshot, the status table (`Mode+StatusSummary`) and AtlasStatus's rules (``Mode`` and ``StatusReading``). Its
/// buttons and ⋯ menu are in `PanelPresenter+Controls`, and its host sessions in `PanelPresenter+Sessions`.
enum PanelPresenter {
  /// The panel as it should look now. While sessions show they take Recent's place, and the line under the heading
  /// names who has the helper or whose action may not have finished; a notice still wins over both.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: Everything the panel draws.
  static func state(_ snapshot: PanelSnapshot) -> PanelState {
    let mode = mode(snapshot)
    let reading = StatusReading(snapshot.status)
    var status = mode.panelSummary(reading, paused: snapshot.session.paused, reconnecting: snapshot.reconnecting)
    if let line = sessionLine(reading, mode: mode) { status.line = line }
    let sessions = sessions(reading, mode: mode, now: snapshot.date)
    return PanelState(
      status: status,
      notice: snapshot.session.notice,
      isMoreOpen: snapshot.session.moreOpen,
      picture: picture(snapshot, mode: mode, reading: reading),
      recent: hidesRecent(mode) || !sessions.isEmpty ? [] : recent(reading, mode: mode),
      sessions: sessions,
      moreSessions: moreSessions(reading, rows: sessions),
      controls: controls(snapshot, mode: mode),
      announcement: snapshot.session.announcement
    )
  }

  /// The helper's mode as the panel and the menu bar glyph show it: connecting while no answer stands, You have
  /// control while Atlas's main window holds it, In use elsewhere while another window does, else the service's
  /// state.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The mode.
  static func mode(_ snapshot: PanelSnapshot) -> Mode {
    if snapshot.connecting { return .connecting }
    return Mode.seen(by: snapshot.status.overview, holdingControl: snapshot.holdingControl)
  }

  /// The picture: the empty stage saying why while the helper is logged out or not ready, the
  /// mark breathing while connecting; else the screen, inside the rim while in control, greyed under the badge while
  /// it needs the person, greyed alone while paused, with the newest action over it while an agent works and the
  /// picture is live, in orange when it failed, led by its session's icon while sessions show. The pause button on it
  /// pauses the picture only.
  ///
  /// - Parameters:
  ///   - snapshot: What the controller holds.
  ///   - mode: The helper's mode.
  ///   - reading: The status, read.
  /// - Returns: The picture's state.
  static func picture(_ snapshot: PanelSnapshot, mode: Mode, reading: StatusReading) -> PanelPictureState {
    let paused = snapshot.session.paused
    let look: PanelPictureState.Look =
      switch mode.kind {
      case .loggedOut: .empty(message: PanelWords.loggedOutPicture)
      // Recover is offered only where it fixes something (``helperMenu``), and the stage says so only then.
      case .notReady:
        .empty(
          message: snapshot.status.overview.recoveryHelps ? PanelWords.notReadyPicture : PanelWords.notReadyWaitPicture
        )
      case .connecting: .connecting
      case .control: .control
      case .needsYou: .needsYou
      case .idle, .working, .stopped, .inUseElsewhere: paused ? .paused : .live
      }
    let newest = mode.kind == .working && !paused ? reading.captionEntries.last : nil
    var state = PanelPictureState(
      look: look,
      caption: newest.map { PanelCaption(text: $0.line, time: $0.when, isFailed: $0.mark == .failed, icon: $0.icon) },
      pause: PanelPause(
        isPaused: paused,
        label: paused ? Words.resumePictureItem : Words.pausePictureItem,
        help: paused ? ShortcutWords.resumePicture : ShortcutWords.pausePicture
      )
    )
    state.picture = state.showsScreen ? snapshot.picture : nil
    return state
  }

  /// Recent's rows, newest first: the newest is live only while an agent works, so only it carries the blue
  /// dot, and only then.
  ///
  /// - Parameters:
  ///   - reading: The status, read.
  ///   - mode: The helper's mode.
  /// - Returns: The rows.
  static func recent(_ reading: StatusReading, mode: Mode) -> [PanelRecentEntry] {
    reading.recentEntries
      .enumerated()
      .map { index, entry in
        PanelRecentEntry(
          id: entry.id,
          line: entry.line,
          when: entry.when,
          isFailed: entry.mark == .failed,
          isLive: mode.kind == .working && index == 0
        )
      }
  }

  /// Whether Recent hides: while the helper is logged out or not ready, as its picture does.
  ///
  /// - Parameter mode: The helper's mode.
  /// - Returns: Whether Recent hides.
  static func hidesRecent(_ mode: Mode) -> Bool {
    mode.kind == .loggedOut || mode.kind == .notReady
  }
}
