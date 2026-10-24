import Testing

@testable import AtlasUI

@Suite("PageSlideEffect")
struct PageSlideEffectTests {
  @Test("going back brings the new page in from the left and sends the old one off to the right")
  func backMirrorsForward() {
    #expect(PageSlideEffect.offset(.willAppear, direction: .forward) == 32)
    #expect(PageSlideEffect.offset(.didDisappear, direction: .forward) == -24)
    #expect(PageSlideEffect.offset(.willAppear, direction: .back) == -32)
    #expect(PageSlideEffect.offset(.didDisappear, direction: .back) == 24)
    #expect(PageSlideEffect.offset(.identity, direction: .back) == 0)
  }
}
