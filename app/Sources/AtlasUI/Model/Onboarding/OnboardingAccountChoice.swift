/// One standard account already on this Mac that the person can pick as the helper.
package struct OnboardingAccountChoice: Equatable, Sendable, Identifiable {
  /// The account the row picks. Picking it sends it.
  package var id: OnboardingAccountPick
  /// The account as people know it.
  package var title: String
  /// What picking it means.
  package var detail: String
  /// What the disc on the left shows.
  package var mark: AccountMark
  /// Whether it is the one picked.
  package var isSelected: Bool

  /// Creates one row of the account choice.
  ///
  /// - Parameters:
  ///   - id: The account the row picks. Picking it sends it.
  ///   - title: The account as people know it.
  ///   - detail: What picking it means.
  ///   - mark: What the disc on the left shows.
  ///   - isSelected: Whether it is the one picked.
  package init(id: OnboardingAccountPick, title: String, detail: String, mark: AccountMark, isSelected: Bool) {
    self.id = id
    self.title = title
    self.detail = detail
    self.mark = mark
    self.isSelected = isSelected
  }
}
