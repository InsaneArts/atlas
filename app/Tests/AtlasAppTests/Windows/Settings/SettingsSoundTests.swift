import AsyncSupportTesting
import AtlasCommands
import AtlasCommandsTesting
import Testing

@testable import AtlasApp
@testable import AtlasUI

@Suite("Settings helper sound")
struct SettingsSoundTests {
  /// What `atlas sound` says while Atlas may not yet record the helper's audio.
  static let needsPermission = "Switch to the helper and allow Atlas when macOS asks."

  @Test("Mute the helper reads as on until the helper answers, then shows why it cannot mute")
  func showsWhyItCannotMute() async {
    let commands = ScriptedCommands([
      "sound": #"{"ok": true, "helperSound": "unavailable", "reason": "\#(Self.needsPermission)", "wanted": true}"#
    ])
    let controller = SettingsController.scripted(commands: commands)
    #expect(controller.state.general.muteHelper)
    #expect(controller.state.general.soundProblem == nil)

    await controller.readHelperSound()

    #expect(controller.state.general.muteHelper, "the person still wants it")
    #expect(controller.state.general.soundProblem == Self.needsPermission)
  }

  @Test("turning Mute the helper off runs atlas sound on and shows the helper's answer")
  func turningTheSwitchOffLetsTheHelperPlay() async throws {
    let commands = ScriptedCommands([
      "sound on": #"{"ok": true, "helperSound": "on", "reason": null, "wanted": false}"#
    ])
    let controller = SettingsController.scripted(commands: commands)
    var general = controller.state.general
    general.muteHelper = false

    controller.update(general)

    try await settle("the change never reached the helper") {
      controller.helperSound == HelperSound(state: .on, wanted: false)
    }
    #expect(commands.ran == [Action.setHelperSound(muted: false).arguments])
    #expect(!controller.state.general.muteHelper)
    #expect(controller.state.general.soundProblem == nil)
  }

  @Test("an older helper's answer shows its upgrade step under the switch")
  func anOlderHelperSaysHowToUpdate() async {
    let older = "The helper is running an older Atlas that cannot mute its apps; run atlas restart, then try again"
    let commands = ScriptedCommands(["sound": #"{"ok": false, "code": "daemon", "error": "\#(older)"}"#])
    let controller = SettingsController.scripted(commands: commands)

    await controller.readHelperSound()

    #expect(controller.state.general.soundProblem == older)
  }

  @Test("sound turned on while an earlier mute is still ending keeps the switch off and says why")
  func anUnfinishedSoundOnKeepsTheSwitchOff() async {
    let ending = "Atlas is still turning off an earlier mute, so apps in the helper may stay silent; it keeps trying."
    let commands = ScriptedCommands([
      "sound on": #"{"ok": true, "helperSound": "unavailable", "reason": "\#(ending)", "wanted": false}"#
    ])
    let controller = SettingsController.scripted(commands: commands)

    await controller.setHelperSound(muted: false)

    #expect(!controller.state.general.muteHelper)
    #expect(controller.state.general.soundProblem == ending)
  }

  @Test("a change the helper did not confirm keeps the person's choice and says it is uncertain")
  func anUnconfirmedChangeSaysSo() async {
    let uncertain = "The helper did not confirm its sound changed; run atlas sound to see"
    let commands = ScriptedCommands([
      "sound on": #"{"ok": false, "code": "daemon", "error": "\#(uncertain)", "uncertain": true}"#
    ])
    let controller = SettingsController.scripted(commands: commands)

    await controller.setHelperSound(muted: false)

    #expect(!controller.state.general.muteHelper, "the switch does not flip back")
    #expect(controller.state.general.soundProblem == uncertain)
  }

  @Test("a change that does not reach the helper keeps the person's choice and says why under the switch")
  func aFailedChangeSaysWhy() async {
    let refusal = "The helper is on the screen; switch back to your own account first"
    let commands = ScriptedCommands([
      "sound off": #"{"ok": false, "code": "on-console", "error": "\#(refusal)"}"#
    ])
    let controller = SettingsController.scripted(commands: commands)

    await controller.setHelperSound(muted: true)

    #expect(controller.state.general.muteHelper)
    #expect(controller.state.general.soundProblem == refusal)
  }
}
