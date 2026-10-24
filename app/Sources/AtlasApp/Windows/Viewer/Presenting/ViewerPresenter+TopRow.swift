import AtlasStatus
import AtlasUI

extension ViewerPresenter {
  /// The top row: the status, the tabs with Activity's unseen dot, the buttons, and the accent wash while this window
  /// holds control.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The top row.
  static func top(_ snapshot: ViewerSnapshot) -> ViewerTopRowState {
    let session = snapshot.session
    return ViewerTopRowState(
      status: snapshot.mode.windowSummary(
        snapshot.reading.statusReading,
        paused: session.paused,
        showing: snapshot.panes.apps.shown?.app
      ),
      tabs: CapsuleTabsState(
        tabs: ViewerTab.allCases.map { CapsuleTab(id: $0, title: ViewerWords.tab($0), symbol: $0.symbol) },
        selected: session.tab,
        label: ViewerWords.tabsLabel
      ),
      activityUnseen: unseen(snapshot),
      actions: actions(snapshot),
      isInControl: snapshot.holdingControl,
      isFullScreen: session.fullScreen
    )
  }

  /// What the Activity tab holds unseen (v3 V2): the actions newer than the newest one seen, orange once one of them
  /// failed; nothing while Activity shows or before the first reading.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: What is unseen.
  static func unseen(_ snapshot: ViewerSnapshot) -> ViewerUnseen {
    guard snapshot.session.tab != .activity, let seen = snapshot.session.activitySeen else { return .none }
    let reading = snapshot.reading.statusReading
    let unseen = reading.activity(limit: .max)
      .prefix { $0.id != seen.newest }
    guard !unseen.isEmpty else { return .none }
    return unseen.contains { !$0.ok } ? .failed : .new
  }

  /// The buttons on the right, only those that apply (v3 D11). Keys only while this window holds control; while the
  /// helper needs the person, Recover… takes the white capsule's place and Stop goes (mac.py:213); while it is logged
  /// out, not ready or still connecting, the white capsule goes and the stage offers what to do; the pause shows while
  /// there is a picture to pause, or one is paused; Clear stands first while a window is outlined (MacShowOnScreen).
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The buttons.
  static func actions(_ snapshot: ViewerSnapshot) -> ViewerTopActions {
    let session = snapshot.session
    let needsYou = needsYou(snapshot)
    let keys: ViewerKeysButton = snapshot.holdingControl ? .shown(isOpen: session.openMenu == .keys) : .hidden
    let agent = needsYou ? nil : agent(snapshot)
    let agentHelp =
      switch agent {
      case .stop: ViewerWords.stopAgentHelp
      case .resume: ViewerWords.resumeAgentHelp
      case nil: ""
      }
    let paused = session.paused
    return ViewerTopActions(
      pause: paused || snapshot.reading.picture != nil
        ? ViewerToggle(
          isOn: paused,
          label: paused ? ViewerWords.resumePicture : ViewerWords.pausePicture,
          help: paused ? ViewerWords.resumePictureHelp : ViewerWords.pausePictureHelp
        ) : nil,
      keys: keys,
      agent: agent,
      agentHelp: agentHelp,
      control: offersControl(snapshot) ? control(snapshot) : nil,
      refusals: session.refusals,
      more: ViewerToggle(isOn: session.openMenu == .more, label: ViewerWords.more, help: ViewerWords.moreHelp),
      details: ViewerToggle(
        isOn: snapshot.detailsShown,
        label: snapshot.detailsShown ? ViewerWords.hideDetails : ViewerWords.showDetails,
        help: snapshot.detailsShown ? ViewerWords.hideDetailsHelp : ViewerWords.showDetailsHelp
      ),
      showsClear: snapshot.panes.apps.shown != nil
    )
  }

  /// Whether the white capsule applies (v3 D11): not while the helper is logged out or still connecting, nor while it
  /// is not ready and control cannot be taken, where there is nothing to take and the stage says what to do instead.
  /// A helper whose state is unknown but which takes control still offers it.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: Whether it shows.
  static func offersControl(_ snapshot: ViewerSnapshot) -> Bool {
    if snapshot.holdingControl { return true }
    return switch snapshot.mode.kind {
    case .loggedOut, .connecting: false
    case .notReady: snapshot.canTakeControl || snapshot.acquiring
    case .idle, .working, .control, .inUseElsewhere, .needsYou, .stopped: true
    }
  }

  /// Whether the helper needs the person and this window holds no control: Recover… is the next step.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: Whether it is Needs you.
  static func needsYou(_ snapshot: ViewerSnapshot) -> Bool {
    snapshot.mode.kind == .needsYou && !snapshot.holdingControl
  }

  /// Stop while an agent acts, as the panel offers it; Resume once stopped, waiting while recovery runs. Neither
  /// while this window holds control: no agent acts then.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The button, or nil for none.
  static func agent(_ snapshot: ViewerSnapshot) -> ViewerAgentButton? {
    snapshot.mode.agentButton(
      snapshot.reading.status.overview,
      holdingControl: snapshot.holdingControl,
      isBusy: snapshot.session.recovering || snapshot.agentRequests > 0
    )
  }

  /// Give Back while in control; Recover… while the helper needs the person; else Connecting… while asked, or Take
  /// Control when it can be taken.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The button.
  static func control(_ snapshot: ViewerSnapshot) -> ViewerControlButton {
    let session = snapshot.session
    if snapshot.holdingControl {
      return ViewerControlButton(
        title: ViewerWords.giveBack,
        isEnabled: true,
        help: ViewerWords.giveBackHelp,
        event: .giveBack
      )
    }
    if needsYou(snapshot) {
      return ViewerControlButton(
        title: ViewerWords.recoverButton,
        isEnabled: !session.recovering && !snapshot.running.contains(.recovery),
        help: ViewerWords.recoverHelp,
        event: .askRecover
      )
    }
    return ViewerControlButton(
      title: snapshot.acquiring ? ViewerWords.connectingControl : ViewerWords.takeControl,
      isEnabled: snapshot.canTakeControl,
      help: ViewerWords.takeControlHelp,
      event: .takeControl
    )
  }
}
