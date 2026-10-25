extension OnboardingFacts {
  /// An account that could be the helper.
  public struct Candidate: Decodable, Equatable, Sendable {
    /// The account's short name.
    public let name: String
    /// The account's full name, when it has one.
    let realName: String?
    /// The account as people know it: its full name, else its short one.
    public var title: String { realName ?? name }

    /// Creates a candidate.
    ///
    /// - Parameters:
    ///   - name: The account's short name.
    ///   - realName: The account's full name.
    public init(name: String, realName: String? = nil) {
      self.name = name
      self.realName = realName
    }
  }
}
