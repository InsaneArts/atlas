import AtlasUI
import CoreGraphics

extension ViewerPresenter {
  /// The Screen tab: the picture and how it is drawn, or why there is none; its chip, the notice, the captions and a
  /// window shown from the Apps tab.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The screen.
  static func screen(_ snapshot: ViewerSnapshot) -> ViewerScreenState {
    let reading = snapshot.reading
    let session = snapshot.session
    let display = reading.frame?.display
    return ViewerScreenState(
      picture: reading.picture,
      look: look(snapshot),
      empty: reading.picture == nil ? empty(snapshot) : nil,
      chip: chip(snapshot),
      notice: session.notice,
      captions: snapshot.holdingControl || session.paused ? [] : reading.statusReading.captionEntries,
      actualSize: session.actualSize && reading.picture != nil
        ? display.map { CGSize(width: $0.width, height: $0.height) } : nil,
      outline: outline(snapshot)
    )
  }

  /// How the picture is drawn: inside control's rim, faded while paused, dimmed while the helper needs the person.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The look.
  static func look(_ snapshot: ViewerSnapshot) -> PictureLook {
    if snapshot.holdingControl { return .inControl }
    if snapshot.session.paused { return .paused }
    return snapshot.mode.kind == .needsYou ? .dimmed : .live
  }

  /// Why there is no picture (v3 V7): logged out, paused before the first, still connecting, or not ready; the first
  /// and the last with the white button that helps. Not ready keeps the last picture behind its words as connecting
  /// does: a helper restarting answers as not ready while it is silent, and the stage should not go blank for it.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The message and what its button sends.
  static func empty(_ snapshot: ViewerSnapshot) -> ViewerEmpty {
    if snapshot.reading.loggedOut {
      return ViewerEmpty(
        message: EmptyMessageState(
          headline: ViewerWords.loggedOutHeadline,
          message: ViewerWords.loggedOutMessage,
          action: EmptyMessageAction(title: HelperWords.switchItem, isPrimary: true)
        ),
        event: .askSwitch
      )
    }
    if snapshot.session.paused {
      return ViewerEmpty(
        message: EmptyMessageState(headline: ViewerWords.pausedHeadline, message: ViewerWords.pausedMessage)
      )
    }
    if snapshot.mode.kind == .connecting {
      return connecting(snapshot.reading)
    }
    return ViewerEmpty(
      message: EmptyMessageState(
        headline: ViewerWords.notReadyHeadline,
        message: ViewerWords.notReadyMessage,
        action: EmptyMessageAction(title: ViewerWords.recoverButton, isPrimary: true)
      ),
      event: .askRecover,
      lastPicture: snapshot.reading.lastPicture
    )
  }

  /// Connecting: the first time, why it takes a moment; again, after a picture was lost, the last picture kept behind
  /// the words with Reconnecting… (v3 X4, R12). The ring round the mark shows once the window has waited a moment.
  ///
  /// - Parameter reading: What the last reading showed.
  /// - Returns: The message, the last picture and the ring.
  static func connecting(_ reading: ViewerReading) -> ViewerEmpty {
    let again = reading.lastPicture != nil
    return ViewerEmpty(
      message: EmptyMessageState(
        headline: again ? ViewerWords.reconnectingHeadline : ViewerWords.connectingHeadline,
        message: again ? "" : ViewerWords.connectingMessage
      ),
      lastPicture: reading.lastPicture,
      showsRing: reading.waitedLong
    )
  }

  /// The chip at the top: what is being typed before control, control before a paused picture.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The chip, or nil for none.
  static func chip(_ snapshot: ViewerSnapshot) -> ViewerScreenChip? {
    if !snapshot.composing.isEmpty { return .typing(snapshot.composing) }
    if snapshot.holdingControl { return .inControl }
    return snapshot.session.paused && snapshot.reading.picture != nil ? .paused : nil
  }
}
