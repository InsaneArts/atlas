import AtlasCommands

/// Closing a deleted account's login that is still open, before setup goes on.
extension OnboardingController {
  /// Close It…: Atlas closes the logins deleted accounts left open, under the person's approval in macOS's dialog.
  /// Once it worked, the close screen says so until Continue; refused or cancelled, it says why and stays.
  func closeLeftovers() async {
    guard snapshot.step == .closeLeftovers else { return }
    guard let outcome = await act(.closeLeftovers, OnboardingWords.closing), outcome.ok else { return }
    snapshot.choices.closedLeftovers = true
  }
}
