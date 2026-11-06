import AtlasService
import Foundation
import Testing

@testable import AtlasCommands

@Suite("HelperSound")
struct HelperSoundTests {
  @Test(
    "the helper's sound reads as muted, on, or unavailable with its fix, beside the choice it keeps",
    arguments: [
      (
        #"{"ok": true, "helperSound": "muted", "reason": null, "wanted": true}"#,
        HelperSound(state: .muted, wanted: true)
      ),
      (#"{"ok": true, "helperSound": "on", "reason": null, "wanted": false}"#, HelperSound(state: .on, wanted: false)),
      (
        #"{"ok": true, "helperSound": "unavailable", "reason": "Switch to the helper and allow Atlas.", "wanted": true}"#,
        HelperSound(state: .unavailable("Switch to the helper and allow Atlas."), wanted: true)
      ),
    ] as [(String, HelperSound)]
  )
  func answers(reply: String, sound: HelperSound) {
    let outcome = Outcome.parse(status: 0, stdout: Data(reply.utf8), stderr: Data())
    #expect(HelperSound.of(outcome) == sound)
  }

  @Test(
    "a failed or unknown answer says nothing about the helper's sound",
    arguments: [
      (1, #"{"ok": false, "code": "daemon", "error": "The helper is not answering"}"#),
      (0, #"{"ok": true, "helperSound": "loud", "wanted": true}"#),
      (0, #"{"ok": true, "helperSound": "on"}"#),
    ] as [(Int32, String)]
  )
  func unknown(status: Int32, reply: String) {
    let outcome = Outcome.parse(status: status, stdout: Data(reply.utf8), stderr: Data())
    #expect(HelperSound.of(outcome) == nil)
  }

  @Test("the status's sound reads as the command's does")
  func statusSound() {
    let said = ViewerStatus.Sound(helperSound: "unavailable", reason: "Allow Atlas.", wanted: true)

    #expect(HelperSound(said) == HelperSound(state: .unavailable("Allow Atlas."), wanted: true))
    #expect(HelperSound(ViewerStatus.Sound(helperSound: "loud", wanted: false)) == nil)
  }

  @Test("sound turned on stays the choice while an earlier mute is still ending")
  func theChoiceStaysWhileUnavailable() {
    let ending = HelperSound(state: .unavailable("Atlas is still turning off an earlier mute."), wanted: false)

    #expect(!ending.wanted)
    #expect(ending.problem == "Atlas is still turning off an earlier mute.")
    #expect(HelperSound(state: .muted, wanted: true).problem == nil)
  }
}
