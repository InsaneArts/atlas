/// One standard account already on this Mac that the person can pick as the helper.
public struct OnboardingAccountChoice: Equatable, Sendable, Identifiable {
  /// The account the row picks. Picking it sends it.
  public var id: OnboardingAccountPick
  /// The account as people know it.
  public var title: String
  /// What picking it means.
  public var detail: String
  /// What the disc on the left shows.
  public var mark: AccountMark
  /// Whether it is the one picked.
  public var isSelected: Bool
  /// What went wrong setting this account up, shown in its row in place of its detail; nil for nothing.
  public var problem: String?

  /// Creates one row of the account choice.
  ///
  /// - Parameters:
  ///   - id: The account the row picks. Picking it sends it.
  ///   - title: The account as people know it.
  ///   - detail: What picking it means.
  ///   - mark: What the disc on the left shows.
  ///   - isSelected: Whether it is the one picked.
  ///   - problem: What went wrong setting this account up; nil, by default, for nothing.
  public init(
    id: OnboardingAccountPick,
    title: String,
    detail: String,
    mark: AccountMark,
    isSelected: Bool,
    problem: String? = nil
  ) {
    self.id = id
    self.title = title
    self.detail = detail
    self.mark = mark
    self.isSelected = isSelected
    self.problem = problem
  }
}
