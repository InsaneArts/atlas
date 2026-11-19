extension VMFacts {
  /// What making a VM does now, as Atlas VM says it.
  public enum Phase: String, Decodable, Sendable {
    /// It checks this Mac has room.
    case checking
    /// macOS comes down from Apple.
    case downloading
    /// macOS is installed into the VM.
    case installing
    /// It is made.
    case done
    /// A word this Atlas does not know.
    case unknown

    /// Creates the phase Atlas VM names: a word not known reads as unknown.
    ///
    /// - Parameter decoder: Atlas VM's word.
    /// - Throws: When the word is not text.
    public init(from decoder: any Decoder) throws {
      let word = try decoder.singleValueContainer()
        .decode(String.self)
      self = Phase(rawValue: word) ?? .unknown
    }
  }
}
