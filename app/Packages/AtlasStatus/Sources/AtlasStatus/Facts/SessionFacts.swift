import AtlasService

/// The helper session's facts the viewer's sidebar lists, each as a value the app words.
public struct SessionFacts: Equatable, Sendable {
  /// The helper's display, in points; nil while unknown.
  public let display: ViewerStatus.Size?
  /// Whether the helper's input is ready.
  public let input: Input
  /// Whether this viewer could take control now, and if not, why.
  let control: Control
  /// Actions of unknown outcome, waiting for recovery.
  let pending: Int
}
