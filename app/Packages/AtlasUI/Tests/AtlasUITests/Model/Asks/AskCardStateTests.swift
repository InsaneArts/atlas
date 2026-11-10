import CoreGraphics
import Testing

@testable import AtlasUI

@Suite("AskCardState")
@MainActor
struct AskCardStateTests {
  @Test("the answers keep their order and always offer Deny, whatever the order they were given in")
  func theAnswersKeepTheirOrder() {
    let ask = AskCardState(id: "a", icon: .mark(.terminal), who: "Codex", waited: "0:01", title: "t", choices: [.once])
    #expect(ask.choices == [.once, .deny])
    #expect(ask.main == .once && ask.others == [.deny])
    let all = AskCardState(
      id: "a",
      icon: .mark(.terminal),
      who: "Codex",
      waited: "0:01",
      title: "t",
      choices: [.deny, .task]
    )
    #expect(all.choices == [.task, .deny])
  }

  @Test("a picture is cut to the well's shape around the element, which it rings where it stands")
  func aPictureIsCutAroundTheElement() throws {
    let picture = try #require(SampleDesktop.chat("ask", stage: .typed))
    let element = PreviewStates.askSendButton
    guard case .picture(let part, let ring)? = AskCardBody.ringed(picture, element: element) else {
      Issue.record("no picture")
      return
    }
    let shape = CGFloat(part.image.width) / CGFloat(part.image.height)
    #expect(abs(shape - AskCardState.pictureAspect) < 0.05)
    #expect(ring.minX >= 0 && ring.maxX <= 1 && ring.minY >= 0 && ring.maxY <= 1, "the element lies inside the part")
    #expect(AskCardBody.ringed(picture, element: CGRect(x: 2, y: 2, width: 0.1, height: 0.1)) == nil)
  }
}
