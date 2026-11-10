import AtlasService
import Foundation
import Testing

@testable import AtlasCommands

@Suite("HomeFiles")
struct HomeFilesTests {
  @Test("the home folder reads as private or open, with what the helper can open and both choices")
  func answers() {
    let reply = #"""
      {"ok": true, "private": false, "readable": 6, "examples": ["dev", ".config"], "offered": false,
       "signInCopies": true}
      """#
    let outcome = Outcome.parse(status: 0, stdout: Data(reply.utf8), stderr: Data())
    let files = HomeFiles(
      isPrivate: false,
      readable: 6,
      examples: ["dev", ".config"],
      offered: false,
      signInCopies: true
    )
    #expect(HomeFiles.of(outcome) == files)
  }

  @Test(
    "a failed or unknown answer says nothing about the home folder",
    arguments: [
      (1, #"{"ok": false, "code": "io", "error": "Atlas couldn't make your home folder private."}"#),
      (0, #"{"ok": true, "private": true}"#),
    ] as [(Int32, String)]
  )
  func unknown(status: Int32, reply: String) {
    let outcome = Outcome.parse(status: status, stdout: Data(reply.utf8), stderr: Data())
    #expect(HomeFiles.of(outcome) == nil)
  }

  @Test("the sign-ins answer says whether copies are allowed, and nothing when it failed")
  func signIns() {
    let allowed = Outcome.parse(status: 0, stdout: Data(#"{"ok": true, "signInCopies": true}"#.utf8), stderr: Data())
    let refused = Outcome.parse(status: 1, stdout: Data(#"{"ok": false, "code": "stopped"}"#.utf8), stderr: Data())
    #expect(SignInCopies.of(allowed) == true)
    #expect(SignInCopies.of(refused) == nil)
  }
}
