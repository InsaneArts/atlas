import AtlasStatus
import AtlasUI

extension ViewerPresenter {
  /// The top row: the status, the sessions button while it shows, the tabs with Activity's unseen dot, the buttons,
  /// and the accent wash while this window holds control.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The top row.
  static func top(_ snapshot: ViewerSnapshot) -> ViewerTopRowState {
    let session = snapshot.session
    return ViewerTopRowState(
      status: status(snapshot),
      sessions: sessionsButton(snapshot),
      tabs: CapsuleTabsState(
        tabs: ViewerTab.allCases.map {
          CapsuleTab(id: $0, title: ViewerWords.tab($0), symbol: $0.symbol, help: ShortcutWords.tab($0))
        },
        selected: session.tab,
        label: ViewerWords.tabsLabel
      ),
      activityUnseen: unseen(snapshot),
      actions: actions(snapshot),
      isInControl: snapshot.holdingControl,
      isFullScreen: session.fullScreen
    )
  }

  /// The status at the row's leading edge: the mode's word and its line, Reconnecting… once a picture was lost. While
  /// an agent works and the Screen shows its captions, the line says nothing, since the newest caption on the picture
  /// says the same; on any other tab, or with no captions, the line carries the action, led by the name of the session
  /// that has the helper (``holderLeads(_:)``). While the helper needs the person and sessions show, the line says
  /// whose action may not have finished.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The status.
  static func status(_ snapshot: ViewerSnapshot) -> StatusSummary {
    let reading = snapshot.reading.statusReading
    var summary = snapshot.mode.windowSummary(
      reading,
      showing: snapshot.panes.apps.shown?.app,
      reconnecting: snapshot.reading.lastPicture != nil
    )
    let captionsSayIt =
      snapshot.mode.kind == .working && snapshot.session.tab == .screen && snapshot.reading.picture != nil
      && !captions(snapshot).isEmpty
    if captionsSayIt { return snapshot.mode.summary(line: "") }
    switch snapshot.mode.kind {
    case .working:
      if let holder = holderLeads(snapshot), !summary.line.isEmpty { summary.line = "\(holder) · \(summary.line)" }
    case .needsYou:
      if let line = reading.causerLine { summary.line = line }
    case .idle, .control, .stopped, .inUseElsewhere, .notReady, .loggedOut, .connecting:
      break
    }
    return summary
  }

  /// The name that leads the working line: the session that has the helper, while sessions show and every session's
  /// work does, only when the line's action is its own, and not while the Screen's chip already names it. Its host's
  /// name alone, so the action keeps the room, unless another live session runs in the same host.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: "Claude Code", the holder's full name, or nil for none.
  static func holderLeads(_ snapshot: ViewerSnapshot) -> String? {
    let reading = snapshot.reading.statusReading
    guard snapshot.shownSessions == .all, let holder = reading.holder,
      let newest = reading.newestAgentAction, newest.sessionKey == holder.key
    else { return nil }
    if snapshot.session.tab == .screen, case .holder = chip(snapshot) { return nil }
    let sharesHost = reading.liveSessions.contains { $0.key != holder.key && $0.host == holder.host }
    return sharesHost ? holder.name : Words.hostName(holder)
  }

  /// What the Activity tab holds unseen: the actions newer than every one seen, orange once one of them
  /// failed, even after it left the status's few, but never for one refused before anything was sent; nothing while
  /// Activity shows or before the first reading.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: What is unseen.
  static func unseen(_ snapshot: ViewerSnapshot) -> ViewerUnseen {
    guard snapshot.session.tab != .activity, let seen = snapshot.session.activitySeen else { return .none }
    let unseen = seen.unseen(in: snapshot.reading.statusReading.activity(limit: .max))
    if seen.failed || unseen.contains(where: \.failed) { return .failed }
    return unseen.isEmpty ? .none : .new
  }

  /// The buttons on the right, only those that apply. Keys only while this window holds control; while the helper
  /// needs the person, Recover… takes the white capsule's place and Stop goes; while it is logged out, not ready or
  /// still connecting, the white capsule goes and the stage offers what to do; the pause shows while there is a picture
  /// to pause, or one is paused, but never while this window holds control, where pausing would leave the person
  /// working blind; ⋯ shows while there is a picture for its commands to act on.
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
      case .stop: ShortcutWords.stopAgent
      case .resume: ShortcutWords.resumeAgent
      case nil: ""
      }
    return ViewerTopActions(
      pause: pause(snapshot),
      keys: keys,
      agent: agent,
      agentHelp: agentHelp,
      control: offersControl(snapshot) ? control(snapshot) : nil,
      refusals: session.refusals,
      more: snapshot.reading.picture == nil
        ? nil : ViewerToggle(isOn: session.openMenu == .more, label: ViewerWords.more, help: ViewerWords.more),
      details: ViewerToggle(
        isOn: snapshot.detailsShown,
        label: snapshot.detailsShown ? ViewerWords.hideDetailsItem : ViewerWords.showDetailsItem,
        help: snapshot.detailsShown ? ShortcutWords.hideDetails : ShortcutWords.showDetails
      )
    )
  }

  /// The picture's pause: while there is a picture to pause, or one is paused, and this window holds no control.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The pause, or nil for none.
  static func pause(_ snapshot: ViewerSnapshot) -> ViewerToggle? {
    let paused = snapshot.session.paused
    guard paused || snapshot.reading.picture != nil, !snapshot.holdingControl else { return nil }
    return ViewerToggle(
      isOn: paused,
      label: paused ? Words.resumePictureItem : Words.pausePictureItem,
      help: paused ? ShortcutWords.resumePicture : ShortcutWords.pausePicture
    )
  }

  /// Whether the white capsule applies: not while the helper is logged out or still connecting, nor while it
  /// is not ready and control cannot be taken, where there is nothing to take and the stage says what to do
  /// instead. A helper whose state is unknown but which takes control still offers it.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: Whether it shows.
  static func offersControl(_ snapshot: ViewerSnapshot) -> Bool {
    if snapshot.holdingControl { return true }
    return switch snapshot.mode.kind {
    case .loggedOut, .connecting: false
    case .notReady: snapshot.canTakeControl || snapshot.canTakeControlOnceResumed || snapshot.acquiring
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
  /// Control when it can be taken, also while the picture is paused, which pressing it resumes first.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The button.
  static func control(_ snapshot: ViewerSnapshot) -> ViewerControlButton {
    let session = snapshot.session
    if snapshot.holdingControl {
      return ViewerControlButton(
        title: ViewerWords.giveBack,
        isEnabled: true,
        help: ShortcutWords.giveBack,
        event: .giveBack,
        titles: ViewerWords.controlTitles
      )
    }
    if needsYou(snapshot) {
      return ViewerControlButton(
        title: ViewerWords.recoverButton,
        isEnabled: !session.recovering && !snapshot.running.contains(.recovery),
        help: ViewerWords.recoverHelp,
        event: .askRecover,
        titles: ViewerWords.controlTitles
      )
    }
    return ViewerControlButton(
      title: snapshot.acquiring ? ViewerWords.connectingControl : ViewerWords.takeControl,
      isEnabled: snapshot.canTakeControl || snapshot.canTakeControlOnceResumed,
      help: ShortcutWords.takeControl,
      event: .takeControl,
      titles: ViewerWords.controlTitles
    )
  }
}
