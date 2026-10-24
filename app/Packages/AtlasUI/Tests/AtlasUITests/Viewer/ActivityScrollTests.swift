import Testing

@testable import AtlasUI

@Suite("ActivityScroll")
struct ActivityScrollTests {
  @Test("a new action arrives at the top and the scroll stays put unless already at the top")
  func theScrollStaysPutUnlessAtTheTop() {
    // At the top, the list holds no line, so it stays at the top and the new line shows as it arrives.
    #expect(ActivityScroll.anchor(scrolledTo: nil, isAtTop: true) == nil)
    #expect(ActivityScroll.anchor(scrolledTo: "38", isAtTop: true) == nil)
    // Scrolled down, it holds the line the person scrolled to, which then keeps its place as lines arrive above it.
    #expect(ActivityScroll.anchor(scrolledTo: "30", isAtTop: false) == "30")
  }
}
