#if DEBUG
  /// One named sample of a window's state, for a preview and the checks over the samples.
  public struct PreviewState<Value>: Identifiable {
    /// The preview's name, unique within its window.
    public let name: String
    /// The state it draws.
    public let value: Value

    /// The name, which identifies the sample.
    public var id: String { name }
  }
#endif
