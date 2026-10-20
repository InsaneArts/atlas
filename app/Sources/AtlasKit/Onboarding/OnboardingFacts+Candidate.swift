extension OnboardingFacts {
  /// An account that could be the helper.
  package struct Candidate: Decodable, Equatable, Sendable {
    /// The account's short name.
    package let name: String
    /// The account's full name, when it has one.
    package let realName: String?
    /// The account as people know it: its full name, else its short one.
    package var title: String { realName ?? name }

    /// Creates a candidate.
    ///
    /// - Parameters:
    ///   - name: The account's short name.
    ///   - realName: The account's full name.
    package init(name: String, realName: String? = nil) {
      self.name = name
      self.realName = realName
    }
  }
}
