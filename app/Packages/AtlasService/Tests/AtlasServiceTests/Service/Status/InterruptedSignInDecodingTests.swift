import AtlasServiceTesting
import Foundation
import Testing

@testable import AtlasService

@Suite("ViewerStatus.InterruptedSignIn")
struct InterruptedSignInDecodingTests {
  @Test("an interrupted sign-in reads its site, kind and whether it can be finished; one Atlas cannot tell is unknown")
  func anInterruptedSignInReads() throws {
    let known = try status([
      "overview": ["signInInterrupted": ["site": "github.com", "kind": "signIn", "canFinish": true, "id": "a1"]]
    ])
    #expect(
      known.overview.signInInterrupted
        == ViewerStatus.InterruptedSignIn(site: "github.com", kind: .signIn, canFinish: true, id: "a1")
    )
    let unknown = try status(["overview": ["signInInterrupted": ["canFinish": false]]])
    #expect(unknown.overview.signInInterrupted == ViewerStatus.InterruptedSignIn())
    #expect(unknown.overview.signInInterrupted?.canFinish == false)
  }

  @Test("no interrupted sign-in, or null, reads as none")
  func noneReadsAsNone() throws {
    #expect(try status(["overview": [String: Any]()]).overview.signInInterrupted == nil)
    #expect(try status(["overview": ["signInInterrupted": NSNull()]]).overview.signInInterrupted == nil)
  }
}
