extension ViewerStatus {
  /// One readiness check.
  public struct Check: Decodable, Equatable, Sendable {
    /// The check's name, as the service gives it.
    public var name: String
    /// Unknown (`nil`) when it cannot be observed yet; never reported as failed.
    public var ok: Bool?
    /// What would make it pass, in the service's words.
    public var nextStep: String?

    /// Creates a check.
    ///
    /// - Parameters:
    ///   - name: The check's name.
    ///   - ok: Whether it passed; `nil` when it cannot be observed yet.
    ///   - nextStep: What would make it pass.
    public init(name: String, ok: Bool?, nextStep: String? = nil) {
      self.name = name
      self.ok = ok
      self.nextStep = nextStep
    }
  }
}
