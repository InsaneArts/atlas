import AtlasService
import AtlasStatus
import AtlasUI

/// Builds the main window's state from what its controller holds. Pure: every choice the window shows is made here,
/// from the snapshot, its rules (`ViewerSnapshot+Rules`, which the controller's guards follow too), the status table
/// (`Mode+StatusSummary`), the shared ``Words`` and the domain packages' (``Mode``, ``StatusReading``,
/// ``ActivityGrouping``, ``ShotGrouping``). Its parts are in `ViewerPresenter+TopRow`, `+Screen`, `+Details`,
/// `+Alert`, `+MoreMenu`, `+Activity`, `+Apps` and `+Shots`.
enum ViewerPresenter {
  /// The window as it should look now.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: Everything the window draws.
  static func state(_ snapshot: ViewerSnapshot) -> ViewerState {
    let tab = snapshot.session.tab
    return ViewerState(
      top: top(snapshot),
      screen: screen(snapshot),
      details: snapshot.detailsShown ? details(snapshot) : nil,
      alert: alert(snapshot),
      // The screen keeps the corner over Activity and Apps, faded while paused; the Screenshots grid has pictures of
      // its own.
      mini: tab.keepsScreenInCorner
        ? snapshot.reading.picture.map { MiniScreenState(picture: $0, isPaused: snapshot.session.paused) } : nil,
      miniCorner: snapshot.session.corner,
      pane: pane(snapshot),
      toast: snapshot.session.toast,
      announcement: snapshot.session.announcement
    )
  }

  /// What shows under the top row: the screen, or the selected tab's pane, made only while it shows.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The pane.
  static func pane(_ snapshot: ViewerSnapshot) -> ViewerPane {
    switch snapshot.session.tab {
    case .screen: .screen
    case .activity: .activity(activity(snapshot))
    case .apps: .apps(apps(snapshot))
    case .screenshots: .screenshots(shots(snapshot))
    }
  }

  /// The helper's state as this window sees it: Connecting before the first reading and while the picture is lost,
  /// unless the helper is known to be logged out, not ready or in need of the person; else the helper's own state,
  /// with this window's control told apart.
  ///
  /// - Parameters:
  ///   - reading: What the last reading showed.
  ///   - holdingControl: Whether this window holds control.
  /// - Returns: The mode.
  static func mode(reading: ViewerReading, holdingControl: Bool) -> Mode {
    guard reading.hasStatus || reading.unavailable != nil else { return .connecting }
    let mode = Mode.seen(by: reading.status.overview, holdingControl: holdingControl)
    guard reading.unavailable != nil else { return mode }
    return switch mode.kind {
    case .idle, .working, .control, .inUseElsewhere: .connecting
    case .needsYou, .stopped, .notReady, .loggedOut, .connecting: mode
    }
  }

  /// The helper as people know it: its full name, the session's user, its short name, or "Helper".
  ///
  /// - Parameter status: The service's status.
  /// - Returns: The name.
  static func account(_ status: ViewerStatus) -> String {
    status.overview.accountName ?? status.session.userName ?? status.overview.account ?? Words.helper
  }

  /// How old the newest picture is at the snapshot's time: "just now", "4 s ago", or a dash before the first.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The picture's age in words.
  static func newest(_ snapshot: ViewerSnapshot) -> String {
    guard snapshot.reading.picture != nil, let frameAt = snapshot.reading.frameAt else { return Words.unknown }
    return Words.ago(max(0, snapshot.now - frameAt))
  }
}
