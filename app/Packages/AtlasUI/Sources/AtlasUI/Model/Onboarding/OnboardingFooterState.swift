/// The bar under every setup screen and the permission panel: Back or Choose Another Account on the left, the track in
/// the middle, the screen's main action on the right. Its defaults show no buttons and no track, as the first reading
/// has.
public struct OnboardingFooterState: Equatable, Sendable {
  /// Where the screen sits on the track in the middle of the bar.
  public struct Track: Equatable, Sendable {
    /// The current screen, from 0.
    public var index: Int
    /// How many screens this side of setup has.
    public var count: Int
    /// What VoiceOver says for the track, such as "Step 2 of 6".
    public var label: String

    /// Creates the track for one screen of a side of setup.
    ///
    /// - Parameters:
    ///   - index: The current screen, from 0.
    ///   - count: How many screens this side of setup has.
    ///   - label: What VoiceOver says for the track, such as "Step 2 of 6".
    public init(index: Int, count: Int, label: String) {
      self.index = index
      self.count = count
      self.label = label
    }
  }

  /// Back, or Choose Another Account in its place, on the left; nil shows none.
  public var back: OnboardingFooterAction?
  /// The screen's main action, on the right; nil shows none.
  public var next: OnboardingFooterAction?
  /// The track; nil shows none.
  public var track: Track?

  /// Creates the bar under a screen.
  ///
  /// - Parameters:
  ///   - back: Back, or Choose Another Account in its place, on the left; nil shows none.
  ///   - next: The screen's main action, on the right; nil shows none.
  ///   - track: The track; nil shows none.
  public init(back: OnboardingFooterAction? = nil, next: OnboardingFooterAction? = nil, track: Track? = nil) {
    self.back = back
    self.next = next
    self.track = track
  }
}
