/// A status at a glance: its name and the tone of its dot.
package struct StatusSummary: Equatable, Sendable {
  /// The status in words, such as "Watching".
  package var label: String
  /// How it reads at a glance, which its dot's colour shows.
  package var tone: StatusTone

  /// Creates a status at a glance.
  ///
  /// - Parameters:
  ///   - label: The status in words, such as "Watching".
  ///   - tone: How it reads at a glance, which its dot's colour shows.
  package init(label: String, tone: StatusTone) {
    self.label = label
    self.tone = tone
  }
}
