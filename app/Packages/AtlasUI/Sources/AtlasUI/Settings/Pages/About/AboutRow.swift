/// The About card's rows, in their displayed order.
enum AboutRow: CaseIterable, Identifiable {
  /// The latest check or install and its action.
  case updates
  /// Reopen the setup walkthrough.
  case gettingStarted

  /// Each row appears once.
  var id: Self { self }
}
