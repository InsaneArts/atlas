extension String {
  /// The first letter, as a disc or a row's mark shows it for a name: empty for an empty name.
  var initial: String {
    String(prefix(1))
  }
}
