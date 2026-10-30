import AtlasService
import AtlasStatus
import AtlasUI
import Foundation

/// Builds the panel's state from what its controller holds. Pure: every choice the panel shows is made here, from
/// the snapshot, the status table (`Mode+StatusSummary`) and AtlasStatus's rules (``Mode`` and ``StatusReading``). Its
/// buttons and ⋯ menu are in `PanelPresenter+Controls`, and its host sessions in `PanelPresenter+Sessions`.
enum PanelPresenter {
  /// The panel as it should look now. While any session is live the sessions take Recent's place, and while two or
  /// more show the line under the heading names who has the helper or whose action may not have finished; a notice
  /// still wins over both.
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
      recent: hidesRecent(mode) || !sessions.isEmpty ? [] : recent(reading, mode: mode, icons: snapshot.icons),
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

  /// 3: the most rows Recent lists.
  static let recentRows = 3

  /// Recent's rows, newest first, each led by its icon: the newest is live only while an agent works, so only it
  /// carries the blue dot, and only then.
  ///
  /// - Parameters:
  ///   - reading: The status, read.
  ///   - mode: The helper's mode.
  ///   - icons: The icons found for Recent's apps.
  /// - Returns: The rows.
  static func recent(_ reading: StatusReading, mode: Mode, icons: [String: IconImage]) -> [PanelRecentEntry] {
    zip(reading.recentEntries, reading.activity(limit: recentRows))
      .enumerated()
      .map { index, pair in
        let (entry, item) = pair
        return PanelRecentEntry(
          id: entry.id,
          icon: recentIcon(item, icons: icons),
          line: entry.line,
          when: entry.when,
          isFailed: entry.mark == .failed,
          isLive: mode.kind == .working && index == 0
        )
      }
  }

  /// What leads one of Recent's rows: Atlas's own mark for what Atlas did itself, recovering or logging out the
  /// helper; the person's mark for their own input; the app's icon, or its tile while none is found, for an agent's
  /// action in an app; and a quiet tile for one in no app, a screenshot's camera or a command's prompt.
  ///
  /// - Parameters:
  ///   - item: The action.
  ///   - icons: The icons found for Recent's apps.
  /// - Returns: The icon.
  static func recentIcon(_ item: ActivityItem, icons: [String: IconImage]) -> ActivityIcon {
    if atlasOperations.contains(item.row.operation) { return .atlas }
    if item.actor == .you { return .you }
    guard item.row.app != nil || item.row.bundleId != nil else {
      return .symbol(item.row.operation == ActivityGrouping.screenshotOperation ? .camera : .terminal)
    }
    let lastPart = item.row.bundleId?
      .split(separator: ".")
      .last
      .map(String.init)
    let name = item.row.app ?? lastPart ?? ""
    let image = AppIconSelection.image(bundleId: item.row.bundleId, name: name, images: icons)
    return .app(.of(image, name: name))
  }

  /// The operations Atlas itself does to the helper: recovering it, and logging it out.
  static let atlasOperations: Set<String> = ["restart", "logout"]

  /// Whether Recent hides: while the helper is logged out or not ready, as its picture does.
  ///
  /// - Parameter mode: The helper's mode.
  /// - Returns: Whether Recent hides.
  static func hidesRecent(_ mode: Mode) -> Bool {
    mode.kind == .loggedOut || mode.kind == .notReady
  }
}
