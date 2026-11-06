import AtlasUI

extension HelperSoundSwitch {
  /// The sound button the panel and the main window draw: the saved choice, and why the sound is not as chosen.
  var button: HelperSoundButton {
    HelperSoundButton(isMuted: isMuted, problem: problem)
  }
}
