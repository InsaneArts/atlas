import Foundation
import Testing

@testable import AtlasKit

@Suite("UpdateAnswer")
struct UpdateAnswerTests {
  @Test(
    "an update check's answer reads as up to date, newer, unpublished or failed",
    arguments: [
      (0, #"{"ok": true, "version": "0.1.0", "latest": "0.1.0", "newer": false}"#, UpdateAnswer.upToDate),
      (0, #"{"ok": true, "version": "0.1.0", "latest": "0.2.0", "newer": true}"#, .newer("0.2.0")),
      // A newer version Install could not name is no answer.
      (0, #"{"ok": true, "version": "0.1.0", "newer": true}"#, .failed),
      // A reply that never says whether one is newer, as `atlas update` without --check gives, is no answer.
      (0, #"{"ok": true, "updated": false, "version": "0.1.0", "available": "Atlas-0.1.0-macos.zip"}"#, .failed),
      (1, #"{"ok": false, "code": "update", "error": "SHA256SUMS does not name an Atlas archive"}"#, .failed),
      (
        1,
        #"{"ok": false, "code": "update", "error": "Atlas has no public release yet. Check https://atlas.insanearts.io for news."}"#,
        .unpublished
      ),
    ] as [(Int32, String, UpdateAnswer)]
  )
  func answers(status: Int32, reply: String, answer: UpdateAnswer) {
    let outcome = Outcome.parse(status: status, stdout: Data(reply.utf8), stderr: Data())
    #expect(UpdateAnswer.of(outcome) == answer)
  }
}
