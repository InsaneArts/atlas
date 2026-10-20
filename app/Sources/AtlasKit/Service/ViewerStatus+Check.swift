extension ViewerStatus {
  /// One readiness check.
  package struct Check: Decodable, Equatable, Sendable {
    /// The check's name, as the service gives it.
    package var name: String
    /// Unknown (`nil`) when it cannot be observed yet; never reported as failed.
    package var ok: Bool?
    /// What would make it pass, in the service's words.
    package var nextStep: String?

    /// Creates a check.
    ///
    /// - Parameters:
    ///   - name: The check's name.
    ///   - ok: Whether it passed; `nil` when it cannot be observed yet.
    ///   - nextStep: What would make it pass.
    package init(name: String, ok: Bool?, nextStep: String? = nil) {
      self.name = name
      self.ok = ok
      self.nextStep = nextStep
    }
  }
}
