import Testing

@testable import AtlasUI

@Suite("ApprovalState")
struct ApprovalStateTests {
  /// A request of `operation` for the VM `work`, asked from a terminal, at `phase`.
  private static func state(_ operation: ApprovalOperation?, _ phase: ApprovalPhase = .asking) -> ApprovalState {
    ApprovalState(operation: operation, desktop: "work", session: "a terminal", phase: phase)
  }

  @Test("an add asks to add the VM by name, its default the action and Escape refusing")
  func addAsks() {
    let card = Self.state(.add).card
    #expect(card.title == "Add the VM “work”?")
    #expect(card.message.hasPrefix("Asked from a terminal."))
    #expect(card.buttons.map(\.id) == [.allow, .refuse])
    #expect(card.buttons.map(\.role) == [.action, .cancel])
    #expect(card.escape == .refuse)
  }

  @Test(
    "a removal or an uninstall asks as a destructive alert, Don't Allow its default",
    arguments: [
      (ApprovalOperation.remove, "Remove the VM “work”?", "Remove VM"),
      (.uninstall, "Uninstall Atlas and its VMs?", "Uninstall"),
    ]
  )
  func destructiveAsks(operation: ApprovalOperation, title: String, action: String) {
    let card = Self.state(operation).card
    #expect(card.title == title)
    #expect(card.buttons.map(\.id) == [.refuse, .allow])
    #expect(card.buttons.map(\.role) == [.safeCancel, .destructive])
    #expect(card.buttons.map(\.title) == ["Don't Allow", action])
    #expect(card.escape == .refuse)
  }

  @Test(
    "a request answered before it was opened again says so with Close alone, never asking again",
    arguments: [
      (ApprovalEarlierAnswer.allowed, "You allowed this already. Nothing is left to answer."),
      (.refused, "You didn't allow this. Nothing changes."),
      (.used, "You allowed this already, and it has gone ahead."),
    ]
  )
  func answeredEarlierCloses(answer: ApprovalEarlierAnswer, message: String) {
    let card = Self.state(.add, .answeredEarlier(answer)).card
    #expect(card.title == "Add the VM “work”?")
    #expect(card.message == message)
    #expect(card.buttons.map(\.id) == [.close])
  }

  @Test("while the request is read, the one way out refuses it")
  func loadingRefusesOnly() {
    let card = ApprovalState.loading.card
    #expect(card.buttons.map(\.id) == [.refuse])
    #expect(card.escape == .refuse)
  }

  @Test("while an answer is kept, its buttons wait and Escape sends nothing")
  func answeringWaits() {
    let card = Self.state(.add, .answering(.allow)).card
    #expect(card.isBusy)
    #expect(card.escape == nil)
  }

  @Test("an answer kept holds with one Done button that closes the window, never one that answers again")
  func answeredCloses() {
    for allowed in [true, false] {
      let card = Self.state(.remove, .answered(allowed: allowed)).card
      #expect(card.shownTitle == (allowed ? "Allowed" : "Not allowed"))
      #expect(card.doneButton?.id == .close)
      #expect(card.escape == .close)
    }
  }

  @Test("an expired request offers Close alone")
  func expiredCloses() {
    let card = Self.state(.add, .expired).card
    #expect(card.title == "This request expired")
    #expect(card.buttons.map(\.id) == [.close])
    #expect(card.escape == .close)
  }

  @Test("a failure says Atlas's reason in place of the message and offers Close alone")
  func failedSaysWhy() {
    let card = Self.state(.add, .failed("This request has expired.")).card
    #expect(card.title == "Add the VM “work”?")
    #expect(card.refusal == "This request has expired.")
    #expect(card.buttons.map(\.id) == [.close])
  }

  @Test("a request that could not be read says so rather than naming any VM")
  func unreadSaysSo() {
    let card = Self.state(nil, .failed("No such request.")).card
    #expect(card.title == "Atlas can't show this request")
    #expect(card.refusal == "No such request.")
  }
}
