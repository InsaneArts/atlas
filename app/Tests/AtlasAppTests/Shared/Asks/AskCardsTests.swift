import AtlasService
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("AskCards")
struct AskCardsTests {
  /// When the questions were asked.
  nonisolated static let asked = Date(timeIntervalSince1970: 1_762_600_000)

  /// A question in Slack.
  ///
  /// - Parameters:
  ///   - kind: What it would do.
  ///   - element: The element's name.
  ///   - role: The element's role.
  ///   - key: The key.
  ///   - typed: How many characters.
  ///   - task: Whether Allow for This Task is offered.
  ///   - once: Whether Just This Once is offered.
  /// - Returns: The question.
  nonisolated static func ask(
    _ kind: Ask.Kind,
    element: String? = nil,
    role: String? = nil,
    key: String? = nil,
    typed: Int = 0,
    task: Bool = true,
    once: Bool = true
  ) -> Ask {
    Ask(
      id: "a1",
      who: "Claude Code · release",
      account: "Slack",
      kind: kind,
      element: element,
      role: role,
      key: key,
      typed: typed,
      offersOnce: once,
      offersTask: task,
      asked: asked,
      ends: asked + 120
    )
  }

  @Test(
    "each kind of question is titled by what it would do there",
    arguments: [
      (ask(.sendKey), "Send a message in Slack"),
      (ask(.newline, typed: 40), "Send a message in Slack"),
      (ask(.click, element: "Merge pull request"), "Click “Merge pull request” in Slack"),
      (ask(.unnamed, role: "AXButton"), "Click a button with no name in Slack"),
      (ask(.unnamed), "Click something with no name in Slack"),
      (ask(.password, typed: 12), "Type in a password field in Slack"),
      (ask(.drag), "Drag in Slack"),
      (ask(.unsure, key: "cmd+e"), "Press Cmd+E in Slack"),
      (ask(.unsure, typed: 1), "Type 1 character in Slack"),
      (ask(.unsure), "Act in Slack"),
    ]
  )
  func eachKindIsTitled(ask: Ask, title: String) {
    #expect(AskWords.title(ask) == title)
  }

  @Test("only the answers a question offers are shown, Deny always, and the main one leads")
  func onlyOfferedAnswersShow() {
    let card = { (ask: Ask) in AskCards.card(ask, status: ViewerStatus(), picture: nil, at: Self.asked) }
    #expect(card(Self.ask(.click)).choices == [.task, .once, .deny])
    #expect(card(Self.ask(.click)).main == .task, "Allow for This Task is the main button")
    #expect(card(Self.ask(.click, once: false)).choices == [.task, .deny])
    #expect(card(Self.ask(.click, task: false)).main == .once)
    #expect(card(Self.ask(.click, task: false)).scope == nil, "no note on what a task covers when it isn't offered")
    #expect(card(Self.ask(.click, task: false, once: false)).choices == [.deny])
    #expect(
      card(Self.ask(.click)).scope == "For this task covers Claude Code in Slack until it finishes or for 1 hour."
    )
  }

  @Test("a password field shows only how many characters, never what was typed")
  func aPasswordShowsItsLength() {
    let card = AskCards.card(Self.ask(.password, typed: 12), status: ViewerStatus(), picture: nil, at: Self.asked)
    #expect(card.body == .note("12 characters in a password field"))
  }

  @Test("how long a question waited, and what was allowed, count on the clock they are read at")
  func theCountdownFollowsTheClock() {
    #expect(AskWords.waited(42) == "0:42")
    #expect(AskWords.waited(125) == "2:05")
    let grant = AllowedGrant(
      session: "k1",
      account: AccountKey(kind: .site, id: "github.com"),
      name: "GitHub",
      ends: Self.asked + 3600
    )
    let list = AskList(allowed: [grant])
    let rows = { (date: Date) in
      AskCards.allowed(status: ViewerStatus(), list: list, at: date)
        .map(\.detail)
    }
    #expect(rows(Self.asked) == ["An agent · 60 min left"])
    #expect(rows(Self.asked + 8 * 60 + 1) == ["An agent · 52 min left"])
    #expect(rows(Self.asked + 3600 - 30) == ["An agent · less than a minute left"])
    #expect(rows(Self.asked + 3600) == [], "it goes once it ends")
  }
}
