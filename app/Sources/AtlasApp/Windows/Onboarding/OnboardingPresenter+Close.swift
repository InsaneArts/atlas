import AtlasOnboarding
import AtlasUI

/// The close screen: a deleted account's login still open, closed under the person's approval before setup goes on.
extension OnboardingPresenter {
  /// The close screen: what is open and what closing it does, or, once it is closed, that setup can go on.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The close screen.
  static func close(_ snapshot: OnboardingSnapshot) -> OnboardingCloseScreenState {
    let closed = snapshot.facts.leftoverLogins.isEmpty
    return OnboardingCloseScreenState(
      title: closed ? OnboardingWords.leftoverClosed : OnboardingWords.leftoverTitle,
      lede: closed ? nil : OnboardingWords.leftoverLede,
      isClosed: closed
    )
  }

  /// The close screen's footer: Close It…, which waits while a command runs, and Continue once it is closed. It has no
  /// place on the track.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The footer.
  static func closeFooter(_ snapshot: OnboardingSnapshot) -> OnboardingFooterState {
    let next =
      snapshot.facts.leftoverLogins.isEmpty
      ? OnboardingFooterAction(title: .continue, event: .continueAfterClose, isEnabled: true, isDefault: true)
      : OnboardingFooterAction(
        title: .closeIt,
        event: .closeLeftovers,
        isEnabled: snapshot.working == nil,
        isDefault: true
      )
    return OnboardingFooterState(next: next)
  }
}
