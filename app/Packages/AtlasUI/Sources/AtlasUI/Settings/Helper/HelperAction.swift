/// A row of the Helper pane's "When something goes wrong" section.
enum HelperAction: CaseIterable, Identifiable {
  /// Recover…, which asks first.
  case recover
  /// Log Out…, which asks first.
  case logOut

  /// Each row is listed once.
  var id: Self { self }
}
