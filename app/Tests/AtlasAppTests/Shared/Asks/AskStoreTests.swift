import AsyncSupportTesting
import AtlasService
import AtlasServiceTesting
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("AskStore")
@MainActor
struct AskStoreTests {
  /// When the clock starts.
  static let start = Date(timeIntervalSince1970: 1_762_600_000)

  /// A question that ends `seconds` after the clock starts.
  ///
  /// - Parameters:
  ///   - id: Its id.
  ///   - seconds: When it ends.
  /// - Returns: The question.
  static func ask(_ id: String, endsAfter seconds: TimeInterval = 120) -> Ask {
    Ask(id: id, who: "Codex", account: "GitHub", kind: .click, asked: start, ends: start + seconds)
  }

  /// A folder whose changes the test makes.
  final class ScriptedFolder {
    /// What hears its changes, once watched.
    var changed: (@MainActor () -> Void)?
    /// The folder.
    var folder: AskFolder {
      AskFolder(
        watch: { [weak self] in
          self?.changed = $0
          return nil
        },
        picture: { _ in nil }
      )
    }
  }

  @Test("the watch reads the questions at once, again at each change, and again when the earliest ends")
  func theWatchReadsOnChangeAndAtExpiry() async throws {
    let service = ScriptedService()
    let clock = ManualClock()
    let store = AskStore(service: service, clock: clock) { Self.start + clock.seconds }
    let folder = ScriptedFolder()
    service.askList = AskList(asks: [Self.ask("a1", endsAfter: 30)])
    store.watch(folder.folder)
    try await settle("the first reading never came") { store.list?.asks.map(\.id) == ["a1"] }
    #expect(service.asksCalls == 1)

    service.askList.asks.append(Self.ask("a2"))
    folder.changed?()
    try await settle("a change was never read") { store.list?.asks.count == 2 }

    try await clock.untilSleeping(for: .seconds(30), "nothing waited for the earliest end")
    // The service takes down a question whose time ran out when it is read.
    service.askList.asks.removeAll { $0.id == "a1" }
    clock.advance(by: .seconds(30))
    try await settle("the question that ended stayed") { store.list?.asks.map(\.id) == ["a2"] }
  }

  @Test("an answer clears the question from the panel and the viewer at once, then goes to the service")
  func anAnswerClearsBothSurfaces() async throws {
    let service = ScriptedService()
    service.askList = AskList(asks: [Self.ask("a1")])
    let actions = HelperActions.scripted(service)
    await actions.asks.read()
    let shown = { (asks: AskList?) -> (AskCardState?, AskCardState?) in
      var panelSnapshot = PanelSnapshot(status: ViewerStatus(), date: Self.start, asks: asks)
      panelSnapshot.status.overview.state = .working
      var viewerSnapshot = ViewerSnapshot()
      viewerSnapshot.reading.hasStatus = true
      viewerSnapshot.reading.status = panelSnapshot.status
      viewerSnapshot.date = Self.start
      viewerSnapshot.asks = asks
      return (PanelPresenter.state(panelSnapshot).ask, ViewerPresenter.screen(viewerSnapshot).ask)
    }
    #expect(shown(actions.asks.list).0?.id == "a1" && shown(actions.asks.list).1?.id == "a1")
    service.hold.insert("asks/answer")
    let answering = Task { await actions.asks.answer("a1", with: .task) }
    try await settle("the answer never reached the service") { service.held["asks/answer"] != nil }
    #expect(shown(actions.asks.list).0 == nil && shown(actions.asks.list).1 == nil, "both clear before the reply")
    service.resume("asks/answer")
    await answering.value
    #expect(service.answers == ["a1 task"])
    #expect(actions.asks.list?.asks.isEmpty == true)
  }

  @Test("a question shows the picture its own reading names, and a rerun's new picture replaces it")
  func aPictureGoesWithItsReading() async throws {
    let service = ScriptedService()
    var first = Self.ask("a1")
    first.picture = "a1-x.jpg"
    service.askList = AskList(asks: [first])
    let store = AskStore(service: service)
    let image = png()
    let folder = AskFolder(watch: { _ in nil }, picture: { _ in image })
    store.watch(folder)
    try await settle("the first picture never came") { store.pictures["a1-x.jpg"] != nil }
    var rerun = first
    rerun.picture = "a1-y.jpg"
    service.askList = AskList(asks: [rerun])
    await store.read()
    #expect(store.pictures["a1-x.jpg"] == nil, "the replaced picture goes at once")
    try await settle("the new picture never came") { store.pictures["a1-y.jpg"] != nil }
  }

  @Test("a reading started before an answer never brings the answered question back")
  func anAnsweredQuestionStaysGone() async throws {
    let service = ScriptedService()
    service.askList = AskList(asks: [Self.ask("a1")])
    let store = AskStore(service: service)
    await store.read()
    service.hold.insert("asks.json")
    let late = Task { await store.read() }
    try await settle("the reading never started") { service.held["asks.json"] != nil }
    service.fail.insert("asks/answer")
    service.askList = AskList(asks: [Self.ask("a1")])
    await store.answer("a1", with: .deny)
    service.resume("asks.json")
    await late.value
    #expect(store.list?.asks.map(\.id) == ["a1"], "a refused answer shows the question as the service has it")
  }
}
