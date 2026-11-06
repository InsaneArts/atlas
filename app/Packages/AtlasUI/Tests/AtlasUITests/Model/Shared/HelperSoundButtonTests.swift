import Testing

@testable import AtlasUI

@Suite("HelperSoundButton")
struct HelperSoundButtonTests {
  @Test("the sound button names what a press does and shows the speaker as the choice stands")
  func namesThePress() {
    let muted = HelperSoundButton(isMuted: true)
    let on = HelperSoundButton(isMuted: false)

    #expect(muted.label == "Unmute the helper")
    #expect(muted.symbol == .speakerMuted)
    #expect(on.label == "Mute the helper")
    #expect(on.symbol == .speakerOn)
    #expect(on.help == on.label)
    #expect(!on.needsAttention)
  }

  @Test("a sound not as chosen keeps the choice, tints the button and says why in its tooltip")
  func tintsWhenUnavailable() {
    let unavailable = HelperSoundButton(isMuted: false, problem: "Atlas is still turning off an earlier mute.")

    #expect(unavailable.symbol == .speakerOn)
    #expect(unavailable.needsAttention)
    #expect(unavailable.help == "Atlas is still turning off an earlier mute.")
    #expect(unavailable.label == "Mute the helper")
  }
}
