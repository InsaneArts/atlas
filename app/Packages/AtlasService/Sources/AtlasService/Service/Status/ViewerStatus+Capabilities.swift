extension ViewerStatus {
  /// What the helper has shown it can do.
  public struct Capabilities: Decodable, Equatable, Sendable {
    /// The last check of the native Save and Open dialogs.
    public var nativeDialogs = NativeDialogs()

    /// Creates capabilities with nothing checked.
    init() {}

    /// The fields of the `capabilities` object this app reads.
    enum CodingKeys: String, CodingKey { case nativeDialogs }

    /// Decodes capabilities, keeping the default of anything missing or of the wrong type.
    ///
    /// - Parameter decoder: The decoder reading the `capabilities` object.
    /// - Throws: `DecodingError` only when it is not an object.
    public init(from decoder: any Decoder) throws {
      let container = try decoder.container(keyedBy: CodingKeys.self)
      nativeDialogs =
        (try? container.decodeIfPresent(NativeDialogs.self, forKey: .nativeDialogs)) ?? NativeDialogs()
    }
  }
}
