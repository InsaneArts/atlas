import AsyncSupportTesting
import AtlasCommands
import AtlasCommandsTesting
import AtlasService
import AtlasServiceTesting
import Testing

@testable import AtlasApp
@testable import AtlasUI

@Suite("Helper sound switch")
struct HelperSoundSwitchTests {
  /// What `atlas sound on` answers.
  static let soundOn = #"{"ok": true, "helperSound": "on", "reason": null, "wanted": false}"#
  /// Why the helper cannot keep its apps silent yet.
  static let needsPermission = "Switch to the helper and allow Atlas when macOS asks."

  @Test("a status reading shows the helper's saved choice and why it cannot hold, as a tinted button")
  func aStatusReadingShowsTheChoice() {
    let sound = HelperSoundSwitch(commands: ScriptedCommands([:]))
    #expect(sound.button == HelperSoundButton(isMuted: true), "muted until the helper says otherwise")

    sound.adopt(HelperSound(state: .unavailable(Self.needsPermission), wanted: true), asked: sound.generation)

    #expect(sound.button == HelperSoundButton(isMuted: true, problem: Self.needsPermission))
    #expect(sound.button.needsAttention)
    #expect(sound.button.help == Self.needsPermission)
  }

  @Test("a status reading never undoes a change under way or one that did not reach the helper")
  func aStatusReadingNeverUndoesAChange() async throws {
    let commands = ScriptedCommands(["sound on": Self.soundOn])
    let gate = ReleaseSignal()
    commands.whileRunning { _ in await gate.wait() }
    let sound = HelperSoundSwitch(commands: commands)

    let change = Task { await sound.set(muted: false) }
    try await settle("the change never started") { commands.ran.count == 1 }
    sound.adopt(HelperSound(state: .muted, wanted: true), asked: sound.generation)
    #expect(!sound.isMuted, "the change under way wins")
    gate.open()
    await change.value

    let refused = HelperSoundSwitch(commands: ScriptedCommands([:]))
    await refused.set(muted: false)
    refused.adopt(HelperSound(state: .muted, wanted: true), asked: refused.generation)
    #expect(!refused.isMuted, "the switch never flips back on a failure")
    #expect(refused.problem != nil)
  }

  @Test("rapid toggles run one atlas sound at a time and end on the person's latest choice")
  func rapidTogglesEndOnTheLatestChoice() async throws {
    let commands = ScriptedCommands([
      "sound off": #"{"ok": true, "helperSound": "muted", "reason": null, "wanted": true}"#,
      "sound on": Self.soundOn,
    ])
    let gate = ReleaseSignal()
    commands.whileRunning { _ in await gate.wait() }
    let sound = HelperSoundSwitch(commands: commands)

    let first = Task { await sound.set(muted: false) }
    try await settle("the first change never started") { commands.ran.count == 1 }
    await sound.set(muted: true)
    await sound.set(muted: false)
    #expect(commands.ran.count == 1, "nothing runs beside the first")
    gate.open()
    await first.value

    let soundOn = Action.setHelperSound(muted: false).arguments
    #expect(commands.ran == [soundOn, soundOn])
    #expect(sound.sound == HelperSound(state: .on, wanted: false))
    #expect(!sound.isMuted)
  }

  @Test("reading the helper's sound during a change reads nothing that could undo it")
  func aReadNeverOverlapsAChange() async throws {
    let commands = ScriptedCommands([
      "sound on": Self.soundOn,
      "sound": #"{"ok": true, "helperSound": "muted", "reason": null, "wanted": true}"#,
    ])
    let gate = ReleaseSignal()
    commands.whileRunning { _ in await gate.wait() }
    let sound = HelperSoundSwitch(commands: commands)

    let change = Task { await sound.set(muted: false) }
    try await settle("the change never started") { commands.ran.count == 1 }
    await sound.read()
    gate.open()
    await change.value

    #expect(commands.ran == [Action.setHelperSound(muted: false).arguments])
    #expect(sound.sound == HelperSound(state: .on, wanted: false))
  }

  @Test("a status asked for before a change is dropped when it arrives after the change ended")
  func aStatusOlderThanAChangeIsDropped() async {
    let sound = HelperSoundSwitch(commands: ScriptedCommands(["sound on": Self.soundOn]))
    let asked = sound.generation

    await sound.set(muted: false)
    sound.adopt(HelperSound(state: .muted, wanted: true), asked: asked)

    #expect(!sound.isMuted)
    sound.adopt(HelperSound(state: .muted, wanted: true), asked: sound.generation)
    #expect(sound.isMuted, "a status asked for after it is shown")
  }

  @Test("a reading the helper answers after a failed change lets status readings show its sound again")
  func aReadClearsAFailedChange() async {
    let commands = ScriptedCommands([
      "sound on": #"{"ok": false, "code": "on-console", "error": "The helper is on the screen"}"#,
      "sound": #"{"ok": true, "helperSound": "muted", "reason": null, "wanted": true}"#,
    ])
    let sound = HelperSoundSwitch(commands: commands)
    await sound.set(muted: false)
    #expect(sound.problem != nil, "the change did not reach the helper")

    await sound.read()
    sound.adopt(HelperSound(state: .on, wanted: false), asked: sound.generation)

    #expect(!sound.isMuted)
    #expect(sound.problem == nil)
  }

  @Test("the panel's sound button turns the helper's sound on through the switch every window shares")
  func thePanelButtonTogglesTheSharedSwitch() async throws {
    let commands = ScriptedCommands(["sound on": Self.soundOn])
    let panel = PanelController.scripted(commands: commands)
    #expect(panel.state.picture.sound == HelperSoundButton(isMuted: true))

    panel.send(.toggleSound)

    try await settle("the change never reached the helper") { panel.actions.sound.sound?.state == .on }
    #expect(commands.ran == [Action.setHelperSound(muted: false).arguments])
    #expect(panel.state.picture.sound == HelperSoundButton(isMuted: false))
  }

  @Test("the main window's sound button shows and changes the same choice as the panel's")
  func theViewerButtonSharesTheChoice() async throws {
    let commands = ScriptedCommands(["sound on": Self.soundOn])
    let viewer = ViewerController.scripted(ScriptedService(), commands: commands)

    viewer.send(.toggleSound)

    try await settle("the change never reached the helper") { viewer.actions.sound.sound?.state == .on }
    #expect(commands.ran == [Action.setHelperSound(muted: false).arguments])
    #expect(viewer.state.footer.controls.sound == HelperSoundButton(isMuted: false))
  }
}
