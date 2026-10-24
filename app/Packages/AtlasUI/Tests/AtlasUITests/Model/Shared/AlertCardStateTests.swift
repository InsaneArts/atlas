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

  @Test("a running alert cannot be dismissed with Escape, nor once it is done, while it waits to close")
  func escapeWaitsWhileBusy() {
    var alert = Self.logOut
    #expect(alert.escape == "cancel")
    #expect(alert.escapeNeedsKey, "Cancel takes Return here, so Escape needs a key of its own")
    alert.phase = .running(label: "Logging Out…")
    #expect(alert.escape == nil)
    alert.phase = .done(title: "Recovered")
    #expect(alert.escape == nil)
  }

  @Test("a running alert shows its word and its work in the button that runs it, and leaves the others as they are")
  func runningShowsInItsButton() throws {
    var alert = Self.recover
    alert.phase = .running(label: "Recovering…")
    let recover = try #require(alert.buttons.first)
    let later = try #require(alert.buttons.last)
    #expect(alert.label(of: recover) == "Recovering…")
    #expect(alert.isWorking(recover))
    #expect(alert.label(of: later) == "Not Now")
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

  @Test("a done alert reads Recovered in its title and in the button that ran it, with no message under it")
  func doneReadsInTitleAndButton() throws {
    var alert = Self.recover
    alert.phase = .done(title: "Recovered")
    let recover = try #require(alert.buttons.first)
    #expect(alert.shownTitle == "Recovered")
    #expect(alert.shownMessage == nil, "no message contradicts Recovered")
    #expect(alert.label(of: recover) == "Recovered")
    #expect(alert.isWorking(recover))
  }
}
