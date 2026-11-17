import AtlasService
import Foundation
import Testing

@testable import AtlasCommands

@Suite("LifecycleAnswer")
struct LifecycleAnswerTests {
  @Test(
    "an answer reads as allowed, refused, or why it was not taken",
    arguments: [
      (0, #"{"ok": true, "answer": "allowed"}"#, LifecycleAnswer.allowed),
      (0, #"{"ok": true, "answer": "refused"}"#, .refused),
      (0, #"{"ok": true}"#, .failed(LifecycleAnswer.unreadable)),
      (
        1,
        #"{"ok": false, "error": "This request was answered already."}"#,
        .failed("This request was answered already.")
      ),
    ] as [(Int32, String, LifecycleAnswer)]
  )
  func answers(status: Int32, reply: String, answer: LifecycleAnswer) {
    let outcome = Outcome.parse(status: status, stdout: Data(reply.utf8), stderr: Data())
    #expect(LifecycleAnswer.of(outcome) == answer)
  }
}
