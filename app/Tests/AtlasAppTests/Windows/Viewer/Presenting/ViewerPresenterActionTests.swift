import AtlasService
import AtlasServiceTesting
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@MainActor
@Suite("ViewerPresenter actions")
struct ViewerPresenterActionTests {
  /// The display the pictures show, in points.
  static let display = ActionFacts.Size(width: 1440, height: 900)

  /// A row of TextEdit's work with its pictures and facts.
  ///
  /// - Parameters:
  ///   - id: The row's id.
  ///   - says: What it did.
  ///   - age: How long before the reading it started.
  ///   - outcome: How it went; worked by default.
  ///   - label: The control or field it named; none by default.
  ///   - facts: What was recorded to mark its picture.
  ///   - after: The picture after it; none by default.
  ///   - afterAt: When the picture after it was taken; unknown by default.
  ///   - session: Whose work it was; Claude Code's by default.
  /// - Returns: The row.
  static func row(
    _ id: String,
    _ says: String,
    age: Double,
    outcome: ActivityLog.Outcome = .worked,
    label: String? = nil,
    facts: ActionFacts,
    after: String? = nil,
    afterAt: Date? = nil,
    session: String = ViewerActivityFixtures.claude
  ) -> ActivityLog.Entry {
    ActivityFixtures.row(
      id: id,
      says: says,
      outcome: outcome,
      ageSeconds: age,
      session: session,
      verb: facts.action?.rawValue,
      app: "TextEdit",
      label: label,
      bundleId: "com.apple.TextEdit",
      before: "\(1_792_665_000 + Int(age))000000000-before.jpg",
      after: after,
      afterAt: afterAt,
      facts: facts
    )
  }

  @Test("a step's picture is marked where the action went, as a share of the display, with what it did")
  func aStepIsMarked() throws {
    let snapshot = try ViewerActivityFixtures.onActivity([
      Self.row(
        "c1",
        "clicked Save in TextEdit",
        age: 3,
        label: "Save",
        facts: ActionFacts(action: .click, outcome: .worked, display: Self.display, point: .init(x: 720, y: 225))
      )
    ])
    let step = try #require(ViewerActivityFixtures.steps(ViewerPresenter.activity(snapshot)).first)
    let mark = try #require(step.picture?.mark)
    #expect(mark.shape == .click(clicks: 1))
    #expect(mark.spot == CGPoint(x: 0.5, y: 0.25))
    #expect(mark.tone == .worked && mark.note == nil)
    #expect(step.picture?.repeats == 1)
  }

  @Test("typing shows what was typed on one line, and a password says so with eight dots and never its length")
  func typingAndPasswords() throws {
    let long = "Release 0.1.1 is out: faster setup and fixes for typing in the helper."
    let snapshot = try ViewerActivityFixtures.onActivity([
      Self.row(
        "c2",
        "typed into Password in TextEdit",
        age: 3,
        label: "Password",
        facts: ActionFacts(action: .type, display: Self.display, point: .init(x: 10, y: 10), secure: true)
      ),
      Self.row(
        "c1",
        "typed into Message in TextEdit",
        age: 5,
        label: "Message",
        facts: ActionFacts(action: .type, display: Self.display, point: .init(x: 10, y: 10), text: long, length: 70)
      ),
    ])
    let steps = ViewerActivityFixtures.steps(ViewerPresenter.activity(snapshot))
    let password = try #require(steps.first)
    #expect(password.words == ViewerStepWords("Typed a password into", target: "Password"))
    #expect(password.picture?.mark?.shape == .typing(text: "••••••••", detail: "A password"))
    #expect(password.notKept, "a password's typing says it was not kept")
    let typed = try #require(steps.last)
    #expect(typed.words.quote == "Release 0.1.1 is out: faster setup…")
    #expect(!typed.notKept)
    #expect(typed.words.text == "Typed “Release 0.1.1 is out: faster setup…” into Message")
    #expect(typed.picture?.mark?.shape == .typing(text: long, detail: "Typed · 70 characters"))
  }

  @Test("an action that failed, may not have finished or sent nothing is marked so, with why")
  func problemsAreMarked() throws {
    let snapshot = try ViewerActivityFixtures.onActivity([
      Self.row(
        "c3",
        "couldn't click Save in TextEdit",
        age: 3,
        outcome: .failed,
        label: "Save",
        facts: ActionFacts(action: .click, outcome: .failed, reason: "no “Save” in this window")
      ),
      Self.row(
        "c2",
        "pressed ⏎ in TextEdit",
        age: 5,
        facts: ActionFacts(action: .key, outcome: .uncertain, keys: [["⏎"]])
      ),
      Self.row(
        "c1",
        "didn't type in TextEdit",
        age: 7,
        outcome: .notSent,
        facts: ActionFacts(action: .type, outcome: .notSent, reason: "another window was in front")
      ),
    ])
    let marks = ViewerActivityFixtures.steps(ViewerPresenter.activity(snapshot))
      .compactMap(\.picture?.mark)
    #expect(marks.map(\.tone) == [.failed, .uncertain, .notSent])
    #expect(marks.map(\.note) == ["No “Save” in this window", "May not have finished", "Another window was in front"])
    #expect(marks.allSatisfy { $0.spot == nil }, "no display, no spot")
  }

  @Test("three scrolls fold into one step whose picture is the first before, counted, its pill saying how far")
  func foldedScrolls() throws {
    let scroll = ActionFacts(
      action: .scroll,
      display: Self.display,
      point: .init(x: 720, y: 450),
      scroll: .init(dx: 0, dy: 3)
    )
    let snapshot = try ViewerActivityFixtures.onActivity([
      Self.row("c3", "scrolled in TextEdit", age: 3, facts: scroll, after: "1792665099500000000-after.jpg"),
      Self.row("c2", "scrolled in TextEdit", age: 4, facts: scroll),
      Self.row("c1", "scrolled in TextEdit", age: 5, facts: scroll),
    ])
    let steps = ViewerActivityFixtures.steps(ViewerPresenter.activity(snapshot))
    let step = try #require(steps.first)
    #expect(steps.count == 1)
    #expect(step.words.count == "×3")
    #expect(step.picture?.repeats == 3)
    #expect(step.picture?.shot.name == "1792665005000000000-before.jpg", "the oldest scroll's")
    #expect(step.picture?.mark?.shape == .scroll(arrow: .down, label: "Down ×3"))
  }

  @Test("a step's picture opens large over the window, steps through each repeat, and compares with the after")
  func largeView() throws {
    let click = ActionFacts(action: .click, display: Self.display, point: .init(x: 720, y: 450))
    var snapshot = try ViewerActivityFixtures.onActivity([
      Self.row("c3", "clicked in TextEdit", age: 3, facts: click, after: "1792665099500000000-after.jpg"),
      Self.row("c2", "clicked in TextEdit", age: 4, facts: click, after: "1792665098000000000-before.jpg"),
      Self.row("c1", "clicked in TextEdit", age: 5, facts: click),
    ])
    let moment = try #require(ViewerPresenter.activityMoments(snapshot)?.moments.first)
    snapshot.panes.activity.action = ViewerActionFocus(moment: moment.id, row: "c1")
    let first = try #require(ViewerPresenter.activity(snapshot).large)
    #expect(first.id == "c1")
    #expect(!first.hasAfter && first.shownCompare == .before, "no picture after it, nothing to compare")
    #expect(!first.canGoBack && first.canGoForward)
    #expect(first.context.hasSuffix("step 1 of 3"))
    #expect(first.mark?.shape == .click(clicks: 1))

    snapshot.panes.activity.action?.row = "c3"
    snapshot.panes.activity.action?.showsChanges = true
    let last = try #require(ViewerPresenter.activity(snapshot).large)
    #expect(last.hasAfter && last.shownCompare == .slide)
    #expect(last.canGoBack && !last.canGoForward)
    #expect(last.changes == nil, "until compared")
    #expect(last.details.map(\.title) == ["What", "Where", "Agent", "Outcome", "Before", "After"])
    #expect(last.details.last?.value == "Taken after the action", "no time where the service gave none")
    #expect(last.afterLabel == "After")

    let key = ViewerActivityFacts.changesKey(
      before: "1792665003000000000-before.jpg",
      after: "1792665099500000000-after.jpg"
    )
    snapshot.panes.activity.changes[key] = [CGRect(x: 0, y: 0, width: 0.5, height: 0.5)]
    #expect(ViewerPresenter.activity(snapshot).large?.changes?.count == 1)

    snapshot.panes.activity.action = nil
    #expect(ViewerPresenter.activity(snapshot).large == nil)
  }

  @Test("a picture Atlas took after an action shows when it was taken and how long after the picture before")
  func afterTime() throws {
    let at = ActivityFixtures.readAt
      .addingTimeInterval(-3)
    let click = ActionFacts(action: .click, display: Self.display, point: .init(x: 720, y: 450), at: at)
    var snapshot = try ViewerActivityFixtures.onActivity([
      Self.row(
        "c2",
        "clicked in TextEdit",
        age: 3,
        facts: click,
        after: "1792665099500000000-after.jpg",
        afterAt: at.addingTimeInterval(1.6)
      ),
      Self.row(
        "c1",
        "clicked in TextEdit",
        age: 5,
        facts: click,
        after: "1792665099500000000-after.jpg",
        afterAt: at.addingTimeInterval(1.02)
      ),
    ])
    let moment = try #require(ViewerPresenter.activityMoments(snapshot)?.moments.first)
    snapshot.panes.activity.action = ViewerActionFocus(moment: moment.id, row: "c2")
    let slower = try #require(ViewerPresenter.activity(snapshot).large)
    let time = ViewerPresenter.hundredths(at.addingTimeInterval(1.6), snapshot)
    #expect(slower.afterLabel == "After · \(time)")
    #expect(slower.details.last?.value == "\(time), 1.6 seconds later")
    snapshot.panes.activity.action?.row = "c1"
    #expect(ViewerPresenter.activity(snapshot).large?.details.last?.value.hasSuffix(", a second later") == true)
  }

  @Test("a replay's frames are each step's marked picture before it, oldest first, then the screen after the last")
  func replay() throws {
    let click = ActionFacts(action: .click, display: Self.display, point: .init(x: 720, y: 450))
    var snapshot = try ViewerActivityFixtures.onActivity([
      Self.row(
        "c2",
        "clicked Send in TextEdit",
        age: 3,
        label: "Send",
        facts: click,
        after: "1792665099500000000-after.jpg"
      ),
      Self.row("c1", "clicked Name in TextEdit", age: 5, label: "Name", facts: click),
    ])
    let moment = try #require(ViewerPresenter.activityMoments(snapshot)?.moments.first)
    let header = ViewerPresenter.activity(snapshot)
      .days
      .flatMap(\.moments)
      .first
    #expect(header?.canReplay == true)
    snapshot.panes.activity.replay = ViewerReplayFocus(moment: moment.id, index: 1)
    let replay = try #require(ViewerPresenter.activity(snapshot).replay)
    #expect(replay.frames.map(\.label) == ["1", "2", "After"])
    #expect(replay.frames.map(\.app) == ["TextEdit", "TextEdit", "TextEdit"], "the screen after takes the last app")
    #expect(replay.appGroups.isEmpty, "one app needs no names over the strip")
    #expect(replay.frames.map(\.words.target) == ["Name", "Send", ""])
    #expect(replay.frames.last?.picture.name == "1792665099500000000-after.jpg")
    #expect(replay.frames.last?.mark == nil)
    #expect(replay.frames.last?.isAfter == true)
    #expect(replay.frames.last?.detail == "TextEdit", "no time where the service gave none")
    #expect(replay.position == "Step 2 of 2")
    #expect(replay.title == "Claude Code · understudy", "titled by its session, which the detail does not repeat")
    #expect(replay.detail.hasPrefix("2 steps"))
    #expect(replay.icon != nil)
  }

  @Test("a click on a known element rings the element, keeping the dot where the pointer landed")
  func elementRinged() throws {
    let element = ActionFacts.Element(frame: .init(x: 360, y: 711, width: 1066, height: 135), name: "Message")
    let snapshot = try ViewerActivityFixtures.onActivity([
      Self.row(
        "c2",
        "scrolled in TextEdit",
        age: 3,
        facts: ActionFacts(action: .scroll, display: Self.display, point: .init(x: 720, y: 450), element: element)
      ),
      Self.row(
        "c1",
        "clicked Message in TextEdit",
        age: 5,
        facts: ActionFacts(action: .click, display: Self.display, point: .init(x: 720, y: 780), element: element)
      ),
    ])
    let marks = ViewerActivityFixtures.steps(ViewerPresenter.activity(snapshot))
      .compactMap(\.picture?.mark)
    let click = try #require(marks.last)
    let area = try #require(click.area)
    #expect(abs(area.minX - 0.25) < 0.0001 && abs(area.minY - 0.79) < 0.0001)
    #expect(abs(area.width - 1066.0 / 1440) < 0.0001 && abs(area.height - 0.15) < 0.0001)
    #expect(click.spot == CGPoint(x: 0.5, y: 780.0 / 900))
    #expect(click.focus == CGPoint(x: area.midX, y: area.midY), "the small picture centres on the element")
    #expect(marks.first?.area == nil, "a scroll keeps its pill at the pointer")
  }

  @Test("with no element, or no display to place it on, the mark keeps a ring of its own size on the spot")
  func pointOnlyWithoutElement() throws {
    let element = ActionFacts.Element(frame: .init(x: 10, y: 10, width: 40, height: 20))
    let snapshot = try ViewerActivityFixtures.onActivity([
      Self.row(
        "c2",
        "clicked in TextEdit",
        age: 3,
        facts: ActionFacts(action: .click, point: .init(x: 20, y: 20), element: element)
      ),
      Self.row(
        "c1",
        "clicked in TextEdit",
        age: 5,
        facts: ActionFacts(action: .click, display: Self.display, point: .init(x: 720, y: 450))
      ),
    ])
    let marks = ViewerActivityFixtures.steps(ViewerPresenter.activity(snapshot))
      .compactMap(\.picture?.mark)
    #expect(marks.map(\.area) == [nil, nil])
    #expect(marks.last?.spot == CGPoint(x: 0.5, y: 0.5))
  }

  @Test("an element partly off the display is ringed where it shows, and one wholly off it is not ringed")
  func elementKeptOnPicture() {
    let display = Self.display
    let partly = ViewerPresenter.share(ActionFacts.Frame(x: -144, y: 450, width: 288, height: 900), of: display)
    #expect(partly == CGRect(x: 0, y: 0.5, width: 0.1, height: 0.5))
    #expect(ViewerPresenter.share(ActionFacts.Frame(x: 1500, y: 10, width: 20, height: 20), of: display) == nil)
  }

  @Test("an action shown large says where it went by app, window and element, and who did it in which task")
  func largeViewNamesWindowAndTask() throws {
    let facts = ActionFacts(
      action: .key,
      display: Self.display,
      keys: [["⏎"]],
      element: ActionFacts.Element(frame: .init(x: 360, y: 711, width: 1066, height: 135), name: "Message"),
      window: "Release notes",
      task: "Post the 0.1.1 release note"
    )
    var snapshot = try ViewerActivityFixtures.onActivity([
      Self.row("k1", "pressed ⏎ in TextEdit", age: 3, facts: facts)
    ])
    let moment = try #require(ViewerPresenter.activityMoments(snapshot)?.moments.first)
    snapshot.panes.activity.action = ViewerActionFocus(moment: moment.id, row: "k1")
    let large = try #require(ViewerPresenter.activity(snapshot).large)
    let details = Dictionary(uniqueKeysWithValues: large.details.map { ($0.title, $0.value) })
    #expect(details["Where"] == "TextEdit › Release notes › Message")
    #expect(details["Agent and task"] == "Claude Code · understudy\nPost the 0.1.1 release note")
    #expect(details["Agent"] == nil)
    #expect(large.context == "Claude Code · understudy · Post the 0.1.1 release note · TextEdit · step 1 of 1")
    #expect(large.icon != nil)
    #expect(large.mark?.area != nil && large.mark?.spot == nil, "keys ring the element with focus")
  }

  @Test("keys pressed say the element with focus after them where Atlas named it, and stand alone where it did not")
  func keysNameTheirElement() throws {
    func pressed(_ id: String, age: Double, element: ActionFacts.Element?) -> ActivityLog.Entry {
      ActivityFixtures.row(
        id: id,
        says: "pressed ⏎ in TextEdit",
        ageSeconds: age,
        session: ViewerActivityFixtures.claude,
        verb: "key",
        app: "TextEdit",
        label: "⏎",
        keys: [["⏎"]],
        facts: ActionFacts(action: .key, display: Self.display, keys: [["⏎"]], element: element)
      )
    }
    let message = ActionFacts.Element(frame: .init(x: 360, y: 711, width: 1066, height: 135), name: "Message")
    let snapshot = try ViewerActivityFixtures.onActivity([
      pressed("k2", age: 3, element: message),
      ActivityFixtures.row(
        id: "c1",
        says: "clicked Save in TextEdit",
        ageSeconds: 4,
        session: ViewerActivityFixtures.claude,
        verb: "click",
        app: "TextEdit"
      ),
      pressed("k1", age: 5, element: nil),
    ])
    let steps = ViewerActivityFixtures.steps(ViewerPresenter.activity(snapshot))
    let named = try #require(steps.first)
    #expect(
      named.words == ViewerStepWords("Pressed", joiner: "in", target: "Message", keys: [["⏎"]], keyName: "Return")
    )
    #expect(named.words.text == "Pressed ⏎ Return in Message")
    let mark = ViewerPresenter.actionMark(pressed("k3", age: 1, element: message), repeats: 1, tone: .worked)
    #expect(mark?.shape == .keys([["⏎"]], name: "Return"), "the mark's keycaps say the name too")
    #expect(steps.last?.words == ViewerStepWords("Pressed", keys: [["⏎"]], keyName: "Return"))
  }

  @Test("a lone key whose cap is a symbol is named, and a shortcut or a key whose cap says its name is not")
  func keyNames() {
    #expect(ViewerWords.keyName([["↩"]]) == "Return")
    #expect(ViewerWords.keyName([["⇥"]]) == "Tab")
    #expect(ViewerWords.keyName([["⎋"]]) == "Esc")
    #expect(ViewerWords.keyName([["⌫"]]) == "Delete")
    #expect(ViewerWords.keyName([["␣"]]) == "Space")
    #expect(ViewerWords.keyName([["↑"]]) == "Up Arrow")
    #expect(ViewerWords.keyName([["→"]]) == "Right Arrow")
    #expect(ViewerWords.keyName([["⌘", "S"]]).isEmpty, "Atlas does not know what a shortcut does")
    #expect(ViewerWords.keyName([["⇧", "↩"]]).isEmpty)
    #expect(ViewerWords.keyName([["↩"], ["↩"]]).isEmpty)
    #expect(ViewerWords.keyName([["Space"]]).isEmpty, "its cap already says it")
    #expect(ViewerWords.keyName([["A"]]).isEmpty)
  }

  @Test("a replay is titled by the task its agent named, the session under it")
  func replayTitledByTask() throws {
    let click = ActionFacts(
      action: .click,
      display: Self.display,
      point: .init(x: 720, y: 450),
      task: "Post the 0.1.1 release note"
    )
    var snapshot = try ViewerActivityFixtures.onActivity([
      Self.row("c1", "clicked Send in TextEdit", age: 3, label: "Send", facts: click)
    ])
    let moment = try #require(ViewerPresenter.activityMoments(snapshot)?.moments.first)
    snapshot.panes.activity.replay = ViewerReplayFocus(moment: moment.id, index: 0)
    let replay = try #require(ViewerPresenter.activity(snapshot).replay)
    #expect(replay.title == "Post the 0.1.1 release note")
    #expect(replay.detail.hasPrefix("Claude Code · understudy · 1 step"))
  }

  @Test("a replay of a session nothing names, with no task, is titled by its app and when it started")
  func replayTitleFallsBack() throws {
    let click = ActionFacts(action: .click, display: Self.display, point: .init(x: 720, y: 450))
    var snapshot = try ViewerActivityFixtures.onActivity([
      Self.row("c1", "clicked Send in TextEdit", age: 3, label: "Send", facts: click, session: "5d0c77aa1e09")
    ])
    let moment = try #require(ViewerPresenter.activityMoments(snapshot)?.moments.first)
    snapshot.panes.activity.replay = ViewerReplayFocus(moment: moment.id, index: 0)
    let replay = try #require(ViewerPresenter.activity(snapshot).replay)
    #expect(replay.title.hasPrefix("TextEdit · "))
    #expect(replay.detail.hasPrefix("Another session · 1 step"))
    #expect(replay.icon == nil)
  }
}
