extension ViewerStatus {
  /// Whether the helper's apps can be heard, as its daemon last said with the status: `muted`, `on` or `unavailable`,
  /// why when unavailable, and the choice the helper keeps.
  public struct Sound: Decodable, Equatable, Sendable {
    /// `muted`, `on` or `unavailable`.
    public var helperSound: String
    /// Why the sound is not as chosen, with its fix; nil otherwise.
    public var reason: String?
    /// Whether the person wants apps in the helper silent.
    public var wanted: Bool

    /// Creates what the daemon says of the helper's sound.
    ///
    /// - Parameters:
    ///   - helperSound: `muted`, `on` or `unavailable`.
    ///   - reason: Why the sound is not as chosen; nil otherwise.
    ///   - wanted: Whether the person wants apps in the helper silent.
    public init(helperSound: String, reason: String? = nil, wanted: Bool) {
      self.helperSound = helperSound
      self.reason = reason
      self.wanted = wanted
    }
  }
}
