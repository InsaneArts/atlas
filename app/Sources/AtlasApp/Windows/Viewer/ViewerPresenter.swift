import AtlasKit
import AtlasUI

/// Builds the viewer's state from what its controller holds. Pure: every choice the window shows is made here, from
/// the snapshot, its rules (`ViewerSnapshot+Rules`, which the controller's guards follow too) and AtlasKit's
/// (``Mode``, ``StatusReading``, ``Words``).
enum ViewerPresenter {
  /// The window as it should look now.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: Everything the window draws.
  static func state(_ snapshot: ViewerSnapshot) -> ViewerState {
    let recovery = snapshot.recovery
    let shown: ViewerRecoveryPrompt? =
      if case .alert(let alert) = recovery {
        ViewerRecoveryPrompt(
          title: alert.title,
          reason: alert.reason,
          isBusy: alert.busy,
          recoverTitle: alert.busy ? ViewerWords.recoveringButton : ViewerWords.recover
        )
      } else {
        nil
      }
    return ViewerState(
      header: header(snapshot),
      stage: stage(snapshot),
      recovery: shown,
      footer: footer(snapshot, recovery: recovery),
      sidebar: sidebar(snapshot)
    )
  }

  // MARK: The parts

  /// The helper's state as this window sees it: Paused, then Reconnecting while the picture is lost, then Connecting
  /// before the first reading, else the helper's own mode with this window's control told apart.
  ///
  /// - Parameters:
  ///   - reading: What the last reading showed.
  ///   - session: What the window is doing.
  /// - Returns: The mode.
  static func mode(reading: ViewerReading, session: ViewerSession) -> Mode {
    if session.paused { return Mode(label: ViewerWords.paused, tone: .quiet, mark: .dimmed) }
    let mode = Mode.seen(by: reading.status.overview, holdingControl: session.holdingControl)
    if reading.unavailable != nil && mode.tone != .attention && !reading.loggedOut {
      return Mode(label: ViewerWords.reconnecting, tone: .quiet, mark: .dimmed)
    }
    return reading.hasStatus || reading.unavailable != nil
      ? mode : Mode(label: ViewerWords.connecting, tone: .quiet, mark: .dimmed)
  }

  /// The helper as people know it: its full name, the session's user, its short name, or "Helper".
  ///
  /// - Parameter status: The service's status.
  /// - Returns: The name.
  static func account(_ status: ViewerStatus) -> String {
    status.overview.accountName ?? status.session.userName ?? status.overview.account ?? ViewerWords.helper
  }

  /// How fresh the picture is: paused, none while it is lost, else its age now.
  ///
  /// - Parameters:
  ///   - reading: What the last reading showed.
  ///   - session: What the window is doing.
  ///   - now: The time now, in seconds of uptime.
  /// - Returns: "Picture paused", "No picture", "Live", "Updated 4 s ago"…
  static func freshness(reading: ViewerReading, session: ViewerSession, now: Double) -> String {
    if session.paused { return ViewerWords.picturePaused }
    if reading.frame == nil && reading.unavailable != nil { return ViewerWords.noPicture }
    return Words.freshness(reading.frameAt.map { now - $0 })
  }

  /// The line beside the footer's buttons: what the person can do now.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The hint.
  static func hint(_ snapshot: ViewerSnapshot) -> String {
    let reading = snapshot.reading
    let session = snapshot.session
    if session.holdingControl {
      return session.keyboardConnected ? ViewerWords.keyboardConnected : ViewerWords.clickToConnect
    }
    if session.acquiring { return ViewerWords.connectingControl }
    if session.paused { return ViewerWords.pausedHint }
    if let blocked = snapshot.controlBlocked { return blocked }
    if reading.unavailable != nil { return ViewerWords.waitingForPicture }
    if reading.hasStatus && !reading.ready && reading.frame != nil { return ViewerWords.inputNotReady }
    return ViewerWords.watching
  }

  /// The header's detail: the helper, then how fresh its picture is at the snapshot's time.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: "Atlas · Live", "Atlas · Updated 4 s ago"…
  static func detail(_ snapshot: ViewerSnapshot) -> String {
    ViewerWords.headerDetail(
      account: account(snapshot.reading.status),
      freshness: freshness(reading: snapshot.reading, session: snapshot.session, now: snapshot.now)
    )
  }

  /// The title bar, with the sidebar's toggle saying what pressing it does.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The header.
  private static func header(_ snapshot: ViewerSnapshot) -> ViewerHeaderState {
    let mode = mode(reading: snapshot.reading, session: snapshot.session)
    let shown = snapshot.sidebarShown
    return ViewerHeaderState(
      status: mode.summary,
      isWorking: mode.mark == .working,
      detail: detail(snapshot),
      sidebar: ViewerToggle(
        isOn: shown,
        label: shown ? ViewerWords.hideSidebar : ViewerWords.showSidebar,
        help: shown ? ViewerWords.hideSidebarHelp : ViewerWords.showSidebarHelp
      ),
      isFullScreen: snapshot.session.fullScreen
    )
  }

  /// The stage: the picture or why there is none, its chip, the notice and the captions.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The stage.
  private static func stage(_ snapshot: ViewerSnapshot) -> ViewerStageState {
    let reading = snapshot.reading
    let session = snapshot.session
    return ViewerStageState(
      picture: reading.picture,
      isDimmed: session.paused,
      isInControl: session.holdingControl,
      placeholder: reading.picture == nil ? placeholder(snapshot) : nil,
      chip: chip(reading: reading, session: session),
      notice: session.notice,
      // The sidebar lists the recent actions too, but whether it shows is the window's width, so the view drops the
      // captions while it draws the sidebar (`ViewerLayout`).
      captions: session.holdingControl || session.paused ? [] : reading.statusReading.captionEntries
    )
  }

  /// Why there is no picture: logged out, unavailable, paused, or still connecting.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The placeholder.
  private static func placeholder(_ snapshot: ViewerSnapshot) -> ViewerStagePlaceholder {
    if snapshot.reading.loggedOut { return .loggedOut(text: Words.loggedOut, canLogIn: snapshot.canLogIn) }
    if let reason = snapshot.reading.unavailable { return .unavailable(reason: reason) }
    return snapshot.session.paused ? .paused : .connecting
  }

  /// The chip at the top: what is being typed before control, control before a paused picture.
  ///
  /// - Parameters:
  ///   - reading: What the last reading showed.
  ///   - session: What the window is doing.
  /// - Returns: The chip, or nil for none.
  private static func chip(reading: ViewerReading, session: ViewerSession) -> ViewerStageChip? {
    if !session.composing.isEmpty { return .typing(session.composing) }
    if session.holdingControl { return .inControl }
    return session.paused && reading.picture != nil ? .paused : nil
  }

  /// The footer: the hint, Recover… when set aside, the pause button saying what pressing it does, Keys and the
  /// control capsule.
  ///
  /// - Parameters:
  ///   - snapshot: What the controller holds.
  ///   - recovery: Where recovery stands.
  /// - Returns: The footer.
  private static func footer(_ snapshot: ViewerSnapshot, recovery: ViewerRecovery) -> ViewerFooterState {
    let session = snapshot.session
    let paused = session.paused
    return ViewerFooterState(
      hint: hint(snapshot),
      offersRecovery: recovery == .setAside,
      pause: ViewerToggle(
        isOn: paused,
        label: paused ? ViewerWords.resumePicture : ViewerWords.pausePicture,
        help: paused ? ViewerWords.resumePictureHelp : ViewerWords.pausePictureHelp
      ),
      keysEnabled: session.holdingControl,
      control: control(snapshot)
    )
  }

  /// Give Back while in control; else Connecting… while asked, or Take Control when it can be taken.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The button.
  private static func control(_ snapshot: ViewerSnapshot) -> ViewerControlButton {
    let session = snapshot.session
    if session.holdingControl {
      return ViewerControlButton(
        title: ViewerWords.giveBack,
        isEnabled: true,
        help: ViewerWords.giveBackHelp,
        event: .giveBack
      )
    }
    return ViewerControlButton(
      title: session.acquiring ? ViewerWords.connectingControl : ViewerWords.takeControl,
      isEnabled: snapshot.canTakeControl,
      help: ViewerWords.takeControlHelp,
      event: .takeControl
    )
  }
}
