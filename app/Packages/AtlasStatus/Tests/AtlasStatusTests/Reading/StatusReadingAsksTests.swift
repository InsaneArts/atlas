import AtlasService
import AtlasServiceTesting
import Foundation
import Testing

@testable import AtlasStatus

@Suite("StatusReading, waiting for you")
struct StatusReadingAsksTests {
  /// The time the tests read at.
  let now = Date(timeIntervalSince1970: 100)

  @Test("before asks.json is read, the status's summary says Waiting for you")
  func theSummaryWaitsBeforeTheList() throws {
    let reading = StatusReading(
      try status(["overview": ["asks": [["id": "a1", "account": "Slack", "what": "send-key", "ends": 200_000]]]])
    )
    #expect(reading.waitsForYou(from: nil, at: now))
    #expect(reading.asksChanged(since: nil))
  }

  @Test("once asks.json is read, its questions are the ones shown, and one that ended is not")
  func theListIsShownOnceRead() throws {
    let reading = StatusReading(
      try status(["overview": ["asks": [["id": "a1", "account": "Slack", "ends": 200_000]]]])
    )
    let open = Ask(id: "a1", who: "Codex", account: "Slack", kind: .click, asked: now, ends: now + 60)
    let ended = Ask(id: "a2", who: "Codex", account: "Slack", kind: .click, asked: now - 120, ends: now - 1)
    let list = AskList(asks: [open, ended])
    #expect(reading.asks(from: list, at: now).map(\.id) == ["a1"])
    #expect(reading.asksChanged(since: list), "the status doesn't list a2")
    #expect(!reading.asksChanged(since: AskList(asks: [open])))
    #expect(!reading.waitsForYou(from: AskList(), at: now), "an answered question clears before the status says so")
  }

  @Test("a grant the status no longer lists reads asks.json again, as when its agent session ended")
  func anEndedGrantReadsAgain() {
    let grant = AllowedGrant(
      session: "k",
      account: AccountKey(kind: .site, id: "github.com"),
      name: "GitHub",
      ends: now + 600
    )
    let reading = StatusReading(ViewerStatus())
    #expect(reading.asksChanged(since: AskList(allowed: [grant])))
    #expect(!reading.asksChanged(since: AskList()))
  }

  @Test("only what the person allowed that still holds is shown")
  func onlyGrantsThatHold() {
    let reading = StatusReading(ViewerStatus())
    let account = AccountKey(kind: .site, id: "github.com")
    let holds = AllowedGrant(session: "k", account: account, name: "GitHub", ends: now + 600)
    let ended = AllowedGrant(session: "j", account: account, name: "GitHub", ends: now)
    #expect(reading.allowed(from: AskList(allowed: [holds, ended]), at: now) == [holds])
  }
}
