extension ViewerStatus.Capabilities {
  /// The last check of the native Save and Open dialogs.
  package struct NativeDialogs: Decodable, Equatable, Sendable {
    /// How the last check stands.
    package enum State: Equatable, Sendable {
      /// Save and Open both completed.
      case passed
      /// A stage did not complete.
      case failed
      /// Anything else the service says, such as `stale` or `unknown`, and a missing state: not checked now.
      case unchecked
    }

    /// How the last check stands; unchecked when the service says nothing this app knows.
    package var state = State.unchecked
    /// Why, in the service's words; empty when it gave none.
    package var reason = ""
    /// How long ago the check ran, in seconds.
    package var ageSeconds: Double?
    /// The first stage that did not pass, when one did not.
    package var failedAt: String?

    /// Creates a check that has not run.
    package init() {}

    /// The fields of the `nativeDialogs` object.
    enum CodingKeys: String, CodingKey { case state, reason, ageSeconds, checks }

    /// One stage of the check, and whether it passed.
    struct Stage: Decodable {
      var stage: String?
      var passed: Bool?
    }

    /// Decodes the check, keeping the default of anything missing or of the wrong type.
    ///
    /// - Parameter decoder: The decoder reading the `nativeDialogs` object.
    /// - Throws: `DecodingError` only when it is not an object.
    package init(from decoder: any Decoder) throws {
      let container = try decoder.container(keyedBy: CodingKeys.self)
      state =
        switch try? container.decodeIfPresent(String.self, forKey: .state) {
        case "passed": .passed
        case "failed": .failed
        default: .unchecked
        }
      reason = (try? container.decodeIfPresent(String.self, forKey: .reason)) ?? ""
      ageSeconds = try? container.decodeIfPresent(Double.self, forKey: .ageSeconds)
      let stages = (try? container.decodeIfPresent([Stage].self, forKey: .checks)) ?? []
      failedAt = stages.first { $0.passed != true }?.stage
    }
  }
}
