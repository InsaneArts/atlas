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

  /// Why there is no picture: logged out, still connecting, paused before the first, or not ready; the first and the
  /// last with the button that helps (MacEmpty).
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The message and what its button sends.
  static func empty(_ snapshot: ViewerSnapshot) -> ViewerEmpty {
    if snapshot.reading.loggedOut {
      return ViewerEmpty(
        message: EmptyMessageState(
          headline: ViewerWords.loggedOutHeadline,
          message: ViewerWords.loggedOutMessage,
          action: EmptyMessageAction(title: HelperWords.switchItem, isPrimary: false)
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
      return ViewerEmpty(
        message: EmptyMessageState(headline: ViewerWords.connectingHeadline, message: ViewerWords.connectingMessage)
      )
    }
    return ViewerEmpty(
      message: EmptyMessageState(
        headline: ViewerWords.notReadyHeadline,
        message: ViewerWords.notReadyMessage,
        action: EmptyMessageAction(title: ViewerWords.recoverButton, isPrimary: true)
      ),
      event: .askRecover
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
