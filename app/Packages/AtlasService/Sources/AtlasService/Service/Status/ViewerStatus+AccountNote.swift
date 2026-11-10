extension ViewerStatus {
  /// The newest account Atlas added after the person typed a password in the helper, for the note that says so once.
  public struct AccountNote: Decodable, Equatable, Sendable {
    /// The account's name, such as `github.com`.
    public var name: String
    /// When it was added, in milliseconds since 1970: the note shows once for each.
    public var added: Double

    /// Creates the note's account.
    ///
    /// - Parameters:
    ///   - name: The account's name.
    ///   - added: When it was added, in milliseconds since 1970.
    public init(name: String, added: Double) {
      self.name = name
      self.added = added
    }
  }
}
