/// A line under a ``GroupedSection``'s group (v3 `.sgfoot`): small print, or a status line saying what runs, what
/// went wrong or what is done.
enum GroupedSectionNote: Equatable, Sendable {
  /// Quiet small print, such as what a list is for.
  case text(String)
  /// A status line with its mark: a spinner for work, an orange "!" for a problem, a green check once done.
  case status(StatusLine.Kind, String)
}
