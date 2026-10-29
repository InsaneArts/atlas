import AtlasStatus
import AtlasUI

extension ViewerPresenter {
  /// The card's footer: the newest thing to know, and the buttons that apply.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The footer.
  static func footer(_ snapshot: ViewerSnapshot) -> ViewerFooterState {
    ViewerFooterState(latest: latest(snapshot), controls: controls(snapshot))
  }

  /// The footer's line, the newest thing to know first: the text being typed, with its caret; why Take Control was
  /// just refused, in orange; a notice, until an action newer than it is done; control, with its blue dot; the latest
  /// action, with its session's icon and its age; else the helper's state's line.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The line.
  static func latest(_ snapshot: ViewerSnapshot) -> ViewerLatestLine {
    let session = snapshot.session
    let reading = snapshot.reading.statusReading
    if !snapshot.composing.isEmpty {
      return ViewerLatestLine(id: LatestID.typing, words: ViewerWords.typing(snapshot.composing), caret: true)
    }
    if let toast = session.toast {
      return ViewerLatestLine(id: LatestID.refusal(session.refusals), words: toast, tone: .attention, announces: true)
    }
    if let notice = session.notice, reading.activity(limit: 1).first?.id == session.noticeAction {
      return ViewerLatestLine(id: LatestID.notice(notice), words: notice, announces: true)
    }
    if snapshot.holdingControl {
      return ViewerLatestLine(id: LatestID.control, words: reading.controlLine, tone: .link)
    }
    if let entry = reading.latestEntry {
      return ViewerLatestLine(
        id: entry.id,
        icon: entry.icon,
        words: entry.line,
        age: entry.when,
        tone: entry.mark == .failed ? .attention : .quiet
      )
    }
    let status = status(snapshot)
    return ViewerLatestLine(id: LatestID.state(status.label), words: status.line)
  }

  /// The footer's buttons, only those that apply. While the helper needs the person, Recover… takes the main
  /// capsule's place and Stop goes; while it is logged out, not ready or still connecting, the main capsule goes and
  /// the stage offers what to do; the pause shows while there is a picture to pause, or one is paused, but never while
  /// this window holds control, where pausing would leave the person working blind.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The buttons.
  static func controls(_ snapshot: ViewerSnapshot) -> ViewerFooterControls {
    let agent = needsYou(snapshot) ? nil : agent(snapshot)
    let agentHelp =
      switch agent {
      case .stop: ShortcutWords.stopAgent
      case .resume: ShortcutWords.resumeAgent
      case nil: ""
      }
    return ViewerFooterControls(
      pause: pause(snapshot),
      agent: agent,
      agentHelp: agentHelp,
      control: offersControl(snapshot) ? control(snapshot) : nil
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

  /// Whether the main capsule applies: not while the helper is logged out or still connecting, nor while it is not
  /// ready and control cannot be taken, where there is nothing to take and the stage says what to do instead. A
  /// helper whose state is unknown but which takes control still offers it.
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

  /// Give Back in the accent while in control; Recover… while the helper needs the person; else Connecting… while
  /// asked, or Take Control when it can be taken, also while the picture is paused, which pressing it resumes first,
  /// each with the hand.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The button.
  static func control(_ snapshot: ViewerSnapshot) -> ViewerControlButton {
    let session = snapshot.session
    if snapshot.holdingControl {
      return ViewerControlButton(
        title: ViewerWords.giveBack,
        look: .accent,
        isEnabled: true,
        help: ShortcutWords.giveBack,
        event: .giveBack,
        labels: mainLabels
      )
    }
    if needsYou(snapshot) {
      return ViewerControlButton(
        title: ViewerWords.recoverButton,
        isEnabled: !session.recovering && !snapshot.running.contains(.recovery),
        help: ViewerWords.recoverHelp,
        event: .askRecover,
        labels: mainLabels
      )
    }
    return ViewerControlButton(
      title: snapshot.acquiring ? ViewerWords.connectingControl : ViewerWords.takeControl,
      symbol: .handRaised,
      isEnabled: snapshot.canTakeControl || snapshot.canTakeControlOnceResumed,
      help: ShortcutWords.takeControl,
      event: .takeControl,
      labels: mainLabels
    )
  }

  /// Every label the main capsule takes in turn, so it keeps the widest one's width through Take Control,
  /// Connecting…, Give Back and Recover… and pressing it never moves it.
  static let mainLabels = [
    ViewerControlLabel(ViewerWords.takeControl, symbol: .handRaised),
    ViewerControlLabel(ViewerWords.connectingControl, symbol: .handRaised),
    ViewerControlLabel(ViewerWords.giveBack),
    ViewerControlLabel(ViewerWords.recoverButton),
  ]
}

/// The footer line's ids, which tell one line from the next: a new id swaps the line, the same one updates it in place.
private enum LatestID {
  /// The text being typed, which changes in place as it is typed.
  static let typing = "typing"
  /// Control, whose words change in place.
  static let control = "control"

  /// A refusal, each one its own line.
  ///
  /// - Parameter count: The refusals counted.
  /// - Returns: The id.
  static func refusal(_ count: Int) -> String {
    "refusal-\(count)"
  }

  /// A notice, each wording its own line.
  ///
  /// - Parameter notice: The notice.
  /// - Returns: The id.
  static func notice(_ notice: String) -> String {
    "notice-\(notice)"
  }

  /// The helper's state, whose line changes in place, such as an age.
  ///
  /// - Parameter label: The state's word.
  /// - Returns: The id.
  static func state(_ label: String) -> String {
    "state-\(label)"
  }
}
