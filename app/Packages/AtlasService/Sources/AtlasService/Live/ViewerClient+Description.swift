extension ViewerClient: CustomStringConvertible, CustomDebugStringConvertible, CustomReflectable {
  /// The mark printed where the capability URL's secret would be.
  static let redacted = "<redacted>"

  /// The client with its capability URL redacted: the URL's path is the secret.
  public var description: String { "ViewerClient(origin: \(origin.debugDescription), base: \(Self.redacted))" }

  /// The same as ``description``, so `String(reflecting:)` never shows the secret either.
  public var debugDescription: String { description }

  /// The client's fields for `dump` and debuggers, with the capability URL redacted.
  public var customMirror: Mirror {
    Mirror(self, children: ["origin": origin, "base": Self.redacted], displayStyle: .struct)
  }
}
