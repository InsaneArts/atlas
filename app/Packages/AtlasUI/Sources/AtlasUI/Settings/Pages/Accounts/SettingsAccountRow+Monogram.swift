extension SettingsAccountRow {
  /// The account's tile: its first letter on its name's tint, as Atlas draws an app without its own icon.
  var monogram: AppMonogram {
    AppMonogram(letter: letter, tint: MonogramTint(name: title))
  }
}
