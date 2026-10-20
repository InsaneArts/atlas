#if DEBUG
  /// One named sample of a window's state, for a preview and the checks over the samples.
  package struct PreviewState<Value>: Identifiable {
    /// The preview's name, unique within its window.
    package let name: String
    /// The state it draws.
    package let value: Value

    /// The name, which identifies the sample.
    package var id: String { name }
  }
#endif
