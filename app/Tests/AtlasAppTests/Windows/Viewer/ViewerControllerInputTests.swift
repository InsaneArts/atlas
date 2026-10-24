import AsyncSupportTesting
import AtlasControl
import AtlasService
import AtlasServiceTesting
import AtlasStatus
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@MainActor
@Suite("ViewerController+Input")
struct ViewerControllerInputTests {
  /// The desktop view: 400 by 300, showing the scripted helper's 1512-point display at about a quarter.
  let size = CGSize(width: 400, height: 300)
  /// Its middle, the helper's point 756, 491.
  let middle = CGPoint(x: 200, y: 150)

  /// The scroll at the view's middle, by `lines` down the page.
  ///
  /// - Parameter lines: The lines, as Atlas sends them.
  /// - Returns: The scroll action.
  func scroll(_ lines: Int) -> InputAction { .scroll(x: 756, y: 491, dx: 0, dy: lines) }

  /// Waits until the viewer, holding control, rests between readings: its heartbeat and its pause between pictures,
  /// both ``ViewerController/controlEvery``, sleep on the clock.
  ///
  /// - Parameters:
  ///   - clock: The viewer's clock.
  ///   - event: What never came, as the failure says it.
  /// - Throws: The failed requirement, when the viewer never rests.
  func resting(_ clock: ManualClock, _ event: String) async throws {
    try await clock.untilSleeping(2, for: ViewerController.controlEvery, event)
  }

  /// Ends the scroll gathering now waiting, as its time passes.
  ///
  /// - Parameter clock: The viewer's clock.
  /// - Throws: The failed requirement, when no scroll is gathering.
  func gathered(_ clock: ManualClock) async throws {
    try await clock.untilSleeping(for: ViewerController.scrollGathering, "no scroll was ever gathered")
    clock.advance(by: ViewerController.scrollGathering)
  }

  @Test("typed text and named keys go to the helper while in control; plain letters are left to the input method")
  func typedTextAndKeysGoToTheHelperWhileInControl() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    await viewer.takeControl()
    viewer.send(text: "héllo")
    #expect(viewer.keyPressed(keyCode: 36, modifiers: []) { nil })
    #expect(!viewer.keyPressed(keyCode: 14, modifiers: []) { nil }, "plain letters are text")
    try await settle("the text and Return were never sent") { service.actions.count == 2 }
    #expect(service.actions == [[.type(text: "héllo")], [.key("return")]])
    viewer.close()
  }

  @Test("Control-Shift-Escape gives control back")
  func controlShiftEscapeGivesControlBack() async throws {
    let viewer = try await ViewerController.watching(ScriptedService())
    await viewer.takeControl()
    #expect(viewer.keyPressed(keyCode: 53, modifiers: [.control, .shift]) { nil })
    #expect(viewer.session.lease == nil)
    viewer.close()
  }

  @Test("scrolling is gathered into one batch")
  func scrollingIsGatheredIntoOneBatch() async throws {
    let service = ScriptedService()
    let clock = ManualClock()
    let viewer = try await ViewerController.watching(service, clock: clock)
    await viewer.takeControl()
    for _ in 0..<3 {
      viewer.scrolled(dx: 0, dy: 10, precise: false, began: false, at: middle, in: size)
    }
    try await gathered(clock)
    try await settle("the gathered scroll was never sent") { service.actions.count == 1 }
    #expect(service.actions.first?.count == 3)
    #expect(service.actions.first?.last == scroll(-30))
    viewer.close()
  }

  @Test("lines past one send's most go on their own, without waiting for another scroll event")
  func linesPastOneSendGoOnTheirOwn() async throws {
    let service = ScriptedService()
    let clock = ManualClock()
    let viewer = try await ViewerController.watching(service, clock: clock)
    await viewer.takeControl()
    viewer.scrolled(dx: 0, dy: 150, precise: false, began: false, at: middle, in: size)
    try await gathered(clock)
    try await settle("the first send never went") { service.actions.count == 1 }
    try await gathered(clock)
    try await settle("the lines past the first send were never sent") { service.actions.count == 2 }
    #expect(service.actions.map(\.last) == [scroll(-100), scroll(-50)])
    viewer.close()
  }

  @Test(
    "a scroll gesture the trackpad begins, or a wheel's turn after a pause, drops the fraction the last one left",
    arguments: [true, false]
  )
  func aNewScrollGestureDropsTheLastFraction(trackpad: Bool) async throws {
    let clock = ManualClock()
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service, clock: clock)
    await viewer.takeControl()
    // A line and a half, then another: three lines in one gesture, two in two.
    let half = trackpad ? 15.0 : 1.5
    viewer.scrolled(dx: 0, dy: half, precise: trackpad, began: false, at: middle, in: size)
    try await gathered(clock)
    try await settle("the first gesture was never sent") { service.actions.count == 1 }
    if !trackpad { clock.advance(by: .seconds(1)) }
    viewer.scrolled(dx: 0, dy: half, precise: trackpad, began: trackpad, at: middle, in: size)
    try await gathered(clock)
    try await settle("the second gesture was never sent") { service.actions.count == 2 }
    #expect(service.actions.map(\.last) == [scroll(-1), scroll(-1)])
    viewer.close()
  }

  @Test("a slow trackpad's fractions add up across pauses within one gesture")
  func aSlowTrackpadsFractionsAddUp() async throws {
    let clock = ManualClock()
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service, clock: clock)
    await viewer.takeControl()
    viewer.scrolled(dx: 0, dy: 15, precise: true, began: true, at: middle, in: size)
    try await gathered(clock)
    try await settle("the first part was never sent") { service.actions.count == 1 }
    clock.advance(by: .seconds(1))
    viewer.scrolled(dx: 0, dy: 15, precise: true, began: false, at: middle, in: size)
    try await gathered(clock)
    try await settle("the second part was never sent") { service.actions.count == 2 }
    #expect(service.actions.map(\.last) == [scroll(-1), scroll(-2)])
    viewer.close()
  }

  @Test("a button's release goes over a stale picture while control is held, so the helper's button comes up")
  func aReleaseGoesOverAStalePicture() async throws {
    let clock = ManualClock()
    let service = ScriptedService()
    // The heartbeat never comes, so control is still held when the button comes up.
    let viewer = try await ViewerController.watching(service, clock: clock, heartbeat: .seconds(86_400))
    await viewer.takeControl()
    viewer.watch(false)
    viewer.pointerDown(.left, at: middle, in: size, modifiers: [])
    try await settle("the press was never sent") { service.actions.count == 1 }
    clock.advance(by: .seconds(Freshness.stale))
    try #require(!viewer.canInput, "the picture is stale")
    viewer.pointerUp(.left, at: middle, in: size, modifiers: [])
    try await settle("the release was never sent") { service.actions.count == 2 }
    #expect(
      service.actions.last == [.pointer(phase: .up, x: 756, y: 491, button: .left, count: 1, modifiers: [])]
    )
    viewer.close()
  }

  @Test("a double-click counts by the Mac's double-click speed and by distances on the view")
  func aDoubleClickCountsByTheMacsSpeedAndTheViewsDistances() async throws {
    let clock = ManualClock()
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service, clock: clock, doubleClick: 2)
    await viewer.takeControl()
    viewer.pointerDown(.left, at: middle, in: size, modifiers: [])
    viewer.pointerUp(.left, at: middle, in: size, modifiers: [])
    // A second later and 8 view points over: 30 of the helper's points, as the view draws it at about a quarter.
    clock.advance(by: .seconds(1))
    viewer.pointerDown(.left, at: CGPoint(x: 208, y: 150), in: size, modifiers: [])
    try await settle("the second press was never sent") { service.actions.count == 3 }
    guard case .pointer(.down, _, _, _, let count, _) = service.actions.last?.first else {
      Issue.record("the second press was not a press")
      return
    }
    #expect(count == 2)
    viewer.close()
  }

  @Test("input that keeps coming reads again at once, then not until the pause between input's readings ends")
  func inputThatKeepsComingReadsAtMostOncePerPause() async throws {
    let service = ScriptedService()
    // The pause between readings ends only as the test moves the clock: only input asks for the readings counted here.
    let clock = ManualClock()
    let viewer = try await ViewerController.watching(service, clock: clock)
    await viewer.takeControl()
    try await resting(clock, "the viewer never came to rest")
    let asked = service.statusCalls
    viewer.inputSent()
    try await settle("the first input never read again") { service.statusCalls == asked + 1 }
    try await resting(clock, "the viewer never came to rest after the first input")
    viewer.inputSent()
    viewer.inputSent()
    #expect(!viewer.readAgain, "input just after a reading waits for the pause")
    try await clock.untilSleeping(for: ViewerController.inputReadEvery, "the input never waited for its pause")
    clock.advance(by: ViewerController.inputReadEvery)
    try await settle("the input that waited never read again") { service.statusCalls == asked + 2 }
    viewer.close()
  }

  @Test("input after a quiet spell reads again at once")
  func inputAfterAQuietSpellReadsAtOnce() async throws {
    let clock = ManualClock()
    let viewer = try await ViewerController.watching(ScriptedService(), clock: clock)
    await viewer.takeControl()
    viewer.inputSent()
    try await resting(clock, "the first input never read again")
    clock.advance(by: ViewerController.inputReadEvery)
    viewer.inputSent()
    #expect(viewer.readAgain)
    #expect(viewer.inputReading == nil, "nothing waits for the pause")
    viewer.close()
  }

  @Test("a paste over the limit is refused, saying the limit")
  func pasteIsLimited() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    await viewer.takeControl()
    viewer.send(text: String(repeating: "a", count: TypedText.pasteLimit + 1))
    #expect(viewer.session.notice == ViewerWords.pasteTooLong(TypedText.pasteLimit))
    // Input leaves in order, so a key sent after the paste arriving alone means the paste sent nothing.
    viewer.sendKey("escape")
    try await settle("the key after the paste was never sent") { !service.actions.isEmpty }
    #expect(service.actions == [[InputAction.key("escape")]])
    viewer.close()
  }
}
