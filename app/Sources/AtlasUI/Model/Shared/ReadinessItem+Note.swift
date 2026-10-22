extension ReadinessItem {
  /// What the check means, under its name; nil when its text is empty, as a ready check's is.
  package var note: String? { text.isEmpty ? nil : text }
}
