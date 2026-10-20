import AtlasUI
import Foundation
import Testing

@testable import AtlasApp
@testable import AtlasKit

@MainActor
@Suite("ViewerController+Input")
struct ViewerControllerInputTests {
  @Test("typed text and named keys go to the helper while in control; plain letters are left to the input method")
  func typedTextAndKeysGoToTheHelperWhileInControl() async {
    let service = ScriptedService()
    let viewer = await ViewerController.watching(service)
    await viewer.takeControl()
    viewer.send(text: "héllo")
    #expect(viewer.keyPressed(keyCode: 36, modifiers: []) { nil })
    #expect(!viewer.keyPressed(keyCode: 14, modifiers: []) { nil }, "plain letters are text")
    await settle { service.actions.count == 2 }
    #expect(service.actions == [[.type(text: "héllo")], [.key("return")]])
    viewer.close()
  }

  @Test("Control-Shift-Escape gives control back")
  func controlShiftEscapeGivesControlBack() async {
    let viewer = await ViewerController.watching(ScriptedService())
    await viewer.takeControl()
    #expect(viewer.keyPressed(keyCode: 53, modifiers: [.control, .shift]) { nil })
    #expect(viewer.session.lease == nil)
    viewer.close()
  }

  @Test("scrolling is gathered into one batch")
  func scrollingIsGatheredIntoOneBatch() async {
    let service = ScriptedService()
    let viewer = await ViewerController.watching(service)
    await viewer.takeControl()
    let size = CGSize(width: 400, height: 300)
    for _ in 0..<3 {
      viewer.scrolled(dx: 0, dy: 10, precise: false, at: CGPoint(x: 200, y: 150), in: size)
    }
    await settle { service.actions.count == 1 }
    #expect(service.actions.first?.count == 3)
    #expect(service.actions.first?.last == .scroll(x: 756, y: 491, dx: 0, dy: -30))
    viewer.close()
  }

  @Test("a paste over the limit is refused, saying the limit")
  func pasteIsLimited() async {
    let service = ScriptedService()
    let viewer = await ViewerController.watching(service)
    await viewer.takeControl()
    viewer.send(text: String(repeating: "a", count: TypedText.pasteLimit + 1))
    #expect(viewer.session.notice == ViewerWords.pasteTooLong(TypedText.pasteLimit))
    // Input is queued as it is sent and leaves the queue only later, so an empty queue now means nothing was sent.
    #expect(viewer.input.pending.isEmpty)
    viewer.close()
  }
}
