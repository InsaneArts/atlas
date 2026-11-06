import AtlasCommands

extension SettingsController {
  /// Whether apps in the helper can be heard, as the switch every window shares last heard.
  var helperSound: HelperSound? { actions.sound.sound }

  /// Reads whether apps in the helper can be heard, through the switch every window shares.
  func readHelperSound() async {
    await actions.sound.read()
  }

  /// Keeps apps in the helper silent, or lets them play, through the switch every window shares.
  ///
  /// - Parameter muted: Whether apps in the helper stay silent.
  func setHelperSound(muted: Bool) async {
    await actions.sound.set(muted: muted)
  }
}
