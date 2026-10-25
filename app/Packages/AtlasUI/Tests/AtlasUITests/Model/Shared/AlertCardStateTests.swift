import Testing

@testable import AtlasUI

@Suite("AlertCardState")
struct AlertCardStateTests {
  /// Log out the helper: Cancel the default, Log Out the destructive action.
  private static let logOut = AlertCardState(
    title: "Log out the helper?",
    message: "Its apps quit.",
    buttons: [
      AlertCardButton(id: "cancel", title: "Cancel", role: .safeCancel),
      AlertCardButton(id: "logOut", title: "Log Out", role: .destructive),
    ]
  )

  /// Recover the helper: Recover the default, Not Now the way out.
  private static let recover = AlertCardState(
    title: "Recover the helper?",
    message: "An action may not have finished.",
    buttons: [
      AlertCardButton(id: "recover", title: "Recover", role: .action),
      AlertCardButton(id: "later", title: "Not Now", role: .cancel),
    ]
  )

  /// Recover once it worked.
  private static let done = AlertCardPhase.done(
    title: "The helper is ready",
    message: "Mouse and keyboard work again.",
    button: "Done"
  )

  @Test("a running alert cannot be dismissed with Escape, but a done one closes with it")
  func escapeWaitsWhileRunning() {
    var alert = Self.logOut
    #expect(alert.escape == "cancel")
    #expect(alert.escapeNeedsKey, "Cancel takes Return here, so Escape needs a key of its own")
    alert.phase = .running(label: "Logging Out…")
    #expect(alert.escape == nil)
    alert.phase = Self.done
    #expect(alert.escape == "cancel")
  }

  @Test("a running alert keeps its button's word beside the work, and VoiceOver hears the running word")
  func runningKeepsItsWord() throws {
    var alert = Self.recover
    alert.phase = .running(label: "Recovering…")
    let recover = try #require(alert.buttons.first)
    let later = try #require(alert.buttons.last)
    #expect(alert.label(of: recover) == "Recover", "the button keeps its width")
    #expect(alert.spokenLabel(of: recover) == "Recovering…")
    #expect(alert.isWorking(recover))
    #expect(alert.label(of: later) == "Not Now")
    #expect(alert.spokenLabel(of: later) == "Not Now")
    #expect(!alert.isWorking(later))
    #expect(alert.isBusy)
  }

  @Test("a refused recovery turns the message into a problem line and brings the buttons back")
  func refusedBringsTheButtonsBack() throws {
    var alert = Self.recover
    alert.phase = .refused(message: "The helper is in use. Try Recover again once it's free.")
    let recover = try #require(alert.buttons.first)
    #expect(alert.refusal == "The helper is in use. Try Recover again once it's free.")
    #expect(!alert.isBusy)
    #expect(!alert.isWorking(recover))
    #expect(alert.label(of: recover) == "Recover")
    #expect(alert.escape == "later")
  }

  @Test("a done alert says the helper is ready, keeps a message, and offers one Done that sends the way out")
  func doneOffersOneButton() throws {
    var alert = Self.recover
    #expect(alert.doneButton == nil)
    alert.phase = Self.done
    #expect(alert.shownTitle == "The helper is ready")
    #expect(alert.shownMessage == "Mouse and keyboard work again.", "the slot keeps words, so the card keeps height")
    let done = try #require(alert.doneButton)
    #expect(done.title == "Done")
    #expect(done.id == "later", "Done closes the alert, as the way out does")
    #expect(done.isDefault, "Return presses it")
    #expect(alert.buttons.allSatisfy { !alert.isWorking($0) }, "no button holds a spinner once done")
  }
}
