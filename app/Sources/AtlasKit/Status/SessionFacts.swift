/// The helper session's facts the viewer's sidebar lists, each as a value the app words.
package struct SessionFacts: Equatable, Sendable {
  /// The helper's display, in points; nil while unknown.
  package let display: ViewerStatus.Size?
  /// Whether the helper's input is ready.
  package let input: Input
  /// Whether this viewer could take control now, and if not, why.
  package let control: Control
  /// Actions of unknown outcome, waiting for recovery.
  package let pending: Int
}
