import AtlasService
import Foundation
import Testing

@testable import AtlasCommands

@Suite("DoctorAnswer")
struct DoctorAnswerTests {
  @Test(
    "a diagnosis reads as ready, as the steps a helper needs, or as failed",
    arguments: [
      (0, #"{"ok": true, "ready": true, "nextSteps": []}"#, DoctorAnswer.ready),
      // The command exits 1 whenever the helper is not ready.
      (1, #"{"ok": true, "ready": false, "nextSteps": ["Restart the Mac."]}"#, .needs(["Restart the Mac."])),
      // A report with no verdict is no answer.
      (0, #"{"ok": true}"#, .failed("")),
      (1, #"{"ok": false, "error": "Run atlas init first"}"#, .failed("Run atlas init first")),
    ] as [(Int32, String, DoctorAnswer)]
  )
  func answers(status: Int32, reply: String, answer: DoctorAnswer) {
    let outcome = Outcome.parse(status: status, stdout: Data(reply.utf8), stderr: Data())
    #expect(DoctorAnswer.of(outcome) == answer)
  }
}
