/// The helper session's facts the viewer's sidebar lists, each as its value in words.
package struct SessionFacts: Equatable, Sendable {
  /// The helper's display, "1512 × 982", or ``Words/unknown``.
  package let display: String
  /// Whether input is ready: "Ready", "Not ready" or "Not answering".
  package let input: String
  /// Whether this viewer could take control now, and if not, why: "Available", "This window", "After recovery"…
  package let control: String
  /// Actions of unknown outcome: a count, or "None".
  package let pending: String
}
