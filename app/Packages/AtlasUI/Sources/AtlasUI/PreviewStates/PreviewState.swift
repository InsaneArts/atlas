#if DEBUG
  import CoreGraphics

  /// One named sample of a window's state, for a preview and the checks over the samples, and for a render, which
  /// names its file by it and draws it at its size.
  public struct PreviewState<Value>: Identifiable {
    /// The preview's name, unique within its window.
    public let name: String
    /// The state it draws.
    public let value: Value
    /// The size a render draws it at; nil for none.
    public let size: CGSize?

    /// The name, which identifies the sample.
    public var id: String { name }

    /// A sample named `name` that draws `value`.
    ///
    /// - Parameters:
    ///   - name: The sample's name, unique within its window.
    ///   - value: The state it draws.
    ///   - size: The size a render draws it at; nil for none.
    public init(name: String, value: Value, size: CGSize? = nil) {
      self.name = name
      self.value = value
      self.size = size
    }
  }
#endif
