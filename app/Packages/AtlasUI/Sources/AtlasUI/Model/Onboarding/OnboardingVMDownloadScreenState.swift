/// macOS coming down from Apple for the VM.
public struct OnboardingVMDownloadScreenState: Equatable, Sendable {
  /// How much is downloaded, from 0 to 1; nil while it is not known.
  public var fraction: Double?
  /// How much is downloaded of the whole, in words, such as "12.4 GB of 25.6 GB"; nil while it is not known.
  public var amount: String?
  /// Whether the add is held, its process ended or failed: the meter keeps its length in grey.
  public var isHeld: Bool
  /// Why the add is held, under the meter; nil while it goes on.
  public var problem: String?

  /// Creates the download's state.
  ///
  /// - Parameters:
  ///   - fraction: How much is downloaded, from 0 to 1; nil while it is not known.
  ///   - amount: How much is downloaded of the whole, in words; nil while it is not known.
  ///   - isHeld: Whether the add is held; false by default.
  ///   - problem: Why the add is held; nil, by default, while it goes on.
  public init(fraction: Double?, amount: String?, isHeld: Bool = false, problem: String? = nil) {
    self.fraction = fraction
    self.amount = amount
    self.isHeld = isHeld
    self.problem = problem
  }
}
