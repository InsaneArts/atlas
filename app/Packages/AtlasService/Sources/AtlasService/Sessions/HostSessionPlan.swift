import Foundation

/// An environment plan a host session applied in the helper. The service keeps no plan id or label for it.
public struct HostSessionPlan: Decodable, Equatable, Sendable {
  /// Where it stands.
  public var phase: HostSessionPlanPhase
  /// When it was made.
  public var madeAt: Date?
  /// Whether it can still be rolled back.
  public var canRollBack: Bool

  /// Creates a plan.
  ///
  /// - Parameters:
  ///   - phase: Where it stands.
  ///   - madeAt: When it was made.
  ///   - canRollBack: Whether it can still be rolled back; not by default.
  public init(phase: HostSessionPlanPhase, madeAt: Date? = nil, canRollBack: Bool = false) {
    self.phase = phase
    self.madeAt = madeAt
    self.canRollBack = canRollBack
  }

  /// The fields of one `plans` entry.
  enum CodingKeys: String, CodingKey {
    /// Each field by the name the service writes.
    case phase, madeAt, canRollBack
  }

  /// Decodes a plan, keeping the default of anything missing or of the wrong type. `madeAt` is read as milliseconds
  /// since 1970.
  ///
  /// - Parameter decoder: The decoder reading one `plans` entry.
  /// - Throws: `DecodingError` only when the entry is not an object.
  public init(from decoder: any Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    phase = HostSessionPlanPhase(word: try? container.decodeIfPresent(String.self, forKey: .phase))
    madeAt = (try? container.decodeIfPresent(Double.self, forKey: .madeAt))
      .flatMap { $0.isFinite ? Date(timeIntervalSince1970: $0 / 1000) : nil }
    canRollBack = (try? container.decodeIfPresent(Bool.self, forKey: .canRollBack)) == true
  }
}
