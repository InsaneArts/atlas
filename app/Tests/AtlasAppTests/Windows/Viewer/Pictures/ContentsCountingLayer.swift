import QuartzCore

/// A layer that counts how often its contents are set, the same image included.
nonisolated final class ContentsCountingLayer: CALayer {
  /// How often the contents were set.
  nonisolated(unsafe) var contentsSets = 0

  override var contents: Any? {
    get { super.contents }
    set {
      contentsSets += 1
      super.contents = newValue
    }
  }
}
