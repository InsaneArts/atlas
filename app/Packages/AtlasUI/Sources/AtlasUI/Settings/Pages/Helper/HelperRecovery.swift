/// A row of the Helper page's "When Something Goes Wrong" section.
enum HelperRecovery: CaseIterable, Identifiable {
  /// Recover…, which asks first.
  case recover
  /// Log Out…, which asks first.
  case logOut

  /// Each row is listed once.
  var id: Self { self }
}
