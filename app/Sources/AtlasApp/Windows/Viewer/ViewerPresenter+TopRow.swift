import AtlasUI

extension ViewerPresenter {
  /// The top row: the status, the tabs and the buttons.
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
        tabs: ViewerTab.allCases.map { CapsuleTab(id: $0, title: ViewerWords.tab($0)) },
        selected: session.tab,
        label: ViewerWords.tabsLabel
      ),
      actions: actions(snapshot),
      isFullScreen: session.fullScreen
    )
  }

  /// The buttons on the right. While the helper needs the person, Recover… takes the white capsule's place and Keys
  /// and Stop go (mac.py:213); Clear stands first while a window is outlined (MacShowOnScreen).
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The buttons.
  static func actions(_ snapshot: ViewerSnapshot) -> ViewerTopActions {
    let session = snapshot.session
    let needsYou = needsYou(snapshot)
    let keys: ViewerKeysButton = needsYou ? .hidden : session.holdingControl ? .pressed : .disabled
    let agent = needsYou ? nil : agent(snapshot)
    let agentHelp =
      switch agent {
      case .stop: ViewerWords.stopAgentHelp
      case .resume: ViewerWords.resumeAgentHelp
      case nil: ""
      }
    return ViewerTopActions(
      pause: ViewerToggle(
        isOn: session.paused,
        label: session.paused ? ViewerWords.resumePicture : ViewerWords.pausePicture,
        help: session.paused ? ViewerWords.resumePictureHelp : ViewerWords.pausePictureHelp
      ),
      keys: keys,
      agent: agent,
      agentHelp: agentHelp,
      control: control(snapshot),
      more: ViewerToggle(isOn: session.openMenu == .more, label: ViewerWords.more, help: ViewerWords.moreHelp),
      details: ViewerToggle(
        isOn: snapshot.detailsShown,
        label: snapshot.detailsShown ? ViewerWords.hideDetails : ViewerWords.showDetails,
        help: snapshot.detailsShown ? ViewerWords.hideDetailsHelp : ViewerWords.showDetailsHelp
      ),
      showsClear: snapshot.panes.apps.shown != nil
    )
  }

  /// Whether the helper needs the person and this window holds no control: Recover… is the next step.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: Whether it is Needs you.
  static func needsYou(_ snapshot: ViewerSnapshot) -> Bool {
    snapshot.mode.kind == .needsYou && !snapshot.session.holdingControl
  }

  /// Stop while an agent acts, as the panel offers it; Resume once stopped, waiting while recovery runs. Neither
  /// while this window holds control: no agent acts then.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The button, or nil for none.
  static func agent(_ snapshot: ViewerSnapshot) -> ViewerAgentButton? {
    snapshot.mode.agentButton(
      snapshot.reading.status.overview,
      holdingControl: snapshot.session.holdingControl,
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
    if session.holdingControl {
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
      title: session.acquiring ? ViewerWords.connectingControl : ViewerWords.takeControl,
      shortTitle: session.acquiring ? ViewerWords.connectingControl : ViewerWords.takeControlShort,
      isEnabled: snapshot.canTakeControl,
      help: ViewerWords.takeControlHelp,
      event: .takeControl
    )
  }
}
