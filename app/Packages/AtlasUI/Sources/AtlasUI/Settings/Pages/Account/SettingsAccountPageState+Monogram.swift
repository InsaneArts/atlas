extension SettingsAccountPageState {
  /// The account's tile: its first letter on its name's tint, the same as its row in the list.
  var monogram: AppMonogram {
    AppMonogram(letter: letter, tint: MonogramTint(name: title))
  }
}
