extension SettingsReadyRow {
  /// What VoiceOver says for the check: Checking while Check Again reads it, its spinner in the mark's place, else
  /// its state.
  var spokenValue: String {
    isChecking ? "Checking" : item.state.spokenValue
  }
}
