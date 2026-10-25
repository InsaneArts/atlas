import AtlasUI
import Testing

@testable import AtlasApp

@Suite("StatusWord")
struct StatusWordTests {
  @Test("every status word starts with a capital and ends without a full stop")
  func everyWordIsCapitalised() {
    for word in StatusWord.allCases {
      let first = word.text.prefix(1)
      #expect(first == first.uppercased(), "\(word)")
      #expect(!word.text.hasSuffix("."), "\(word)")
    }
  }

  @Test("a word that worked is green, a real problem orange, and waiting or checking quiet")
  func eachWordHasOneTone() {
    #expect(StatusWord.allowed.tone == .ready)
    #expect(StatusWord.passed.tone == .ready)
    #expect(StatusWord.failed.tone == .attention)
    #expect(StatusWord.turnedOff.tone == .attention)
    #expect(StatusWord.next.tone == .quiet)
    #expect(StatusWord.waitingForYou.tone == .quiet)
    #expect(StatusWord.checking.tone == .quiet)
  }
}
