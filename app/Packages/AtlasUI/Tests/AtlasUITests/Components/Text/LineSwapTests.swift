import SwiftUI
import Testing

@testable import AtlasUI

@Suite("LineSwap")
struct LineSwapTests {
  /// A swap of words, the line kind the rules are read through.
  private typealias Words = LineSwap<String, String, Text>
  /// The queue of a swap of words.
  private typealias Queue = LineSwapQueue<String>

  @Test("a new line waits for the old one to fade out before it fades in")
  func aNewLineWaitsForTheOldOneToFadeOut() {
    let leaving = Queue(shown: "Clicked Ring")
      .next(.arrived("Scrolled"))
    #expect(leaving == Queue(shown: "Clicked Ring", waiting: "Scrolled", phase: .fadingOut))
    let arrived = leaving.next(.fadedOut)
    #expect(arrived == Queue(shown: "Scrolled", waiting: nil, phase: .holding))
  }

  @Test("a line that arrives while another is fading out replaces the one waiting")
  func theNewestLineReplacesTheOneWaiting() {
    let queue = Queue(shown: "Clicked Ring")
      .next(.arrived("Scrolled"))
      .next(.arrived("Typed"))
    #expect(queue == Queue(shown: "Clicked Ring", waiting: "Typed", phase: .fadingOut))
    #expect(queue.next(.fadedOut).shown == "Typed", "the line in between is skipped")
  }

  @Test("a line holds for at least 0.6 seconds before the next replaces it")
  func aLineHoldsBeforeTheNextReplacesIt() {
    #expect(Words.hold == 0.6)
    let holding = Queue(shown: "Scrolled", waiting: nil, phase: .holding)
    let waiting = holding.next(.arrived("Typed"))
      .next(.arrived("Clicked"))
    #expect(waiting == Queue(shown: "Scrolled", waiting: "Clicked", phase: .holding), "nothing fades during the hold")
    #expect(waiting.next(.held) == Queue(shown: "Scrolled", waiting: "Clicked", phase: .fadingOut))
    // A line whose hold ended with nothing waiting rests, and the next one replaces it at once.
    let resting = holding.next(.held)
    #expect(resting == Queue(shown: "Scrolled", waiting: nil, phase: .resting))
    #expect(resting.next(.arrived("Typed")).phase == .fadingOut)
  }

  @Test("the drawn line arriving again with new details is never a swap")
  func theDrawnLineArrivingAgainIsNoSwap() {
    let resting = Queue(shown: "Scrolled")
    #expect(resting.next(.arrived("Scrolled")) == resting)
    let holding = Queue(shown: "Scrolled", waiting: "Typed", phase: .holding)
    #expect(holding.next(.arrived("Scrolled")) == Queue(shown: "Scrolled", waiting: nil, phase: .holding))
  }

  @Test("a line that comes back while it fades out turns round and holds again")
  func aLineComingBackTurnsItsFadeRound() {
    let leaving = Queue(shown: "Scrolled", waiting: "Typed", phase: .fadingOut)
    #expect(leaving.next(.arrived("Scrolled")) == Queue(shown: "Scrolled", waiting: nil, phase: .holding))
  }

  @Test("a hold or a fade that ends after the swap moved on changes nothing")
  func lateEndsChangeNothing() {
    let resting = Queue(shown: "Scrolled")
    #expect(resting.next(.held) == resting)
    #expect(resting.next(.fadedOut) == resting)
    let leaving = Queue(shown: "Scrolled", waiting: "Typed", phase: .fadingOut)
    #expect(leaving.next(.held) == leaving)
  }

  @Test("under Reduce Motion a line swap still only fades")
  func aLineSwapOnlyFades() {
    #expect(Words.fade(isShowing: true) == Motion.swapIn)
    #expect(Words.fade(isShowing: false) == Motion.swapOut)
    #expect(Motion.Duration.swapIn.seconds <= Motion.Duration.reduced.seconds)
    #expect(Motion.Duration.swapOut.seconds <= Motion.Duration.reduced.seconds)
  }
}
