/// A phrase's first letter, raised to lead a line or lowered to follow a word, as the service's words and the app's own
/// are joined.
extension Words {
  /// The phrase with its first letter in capitals, to lead a line.
  ///
  /// - Parameter phrase: The phrase, such as "clicked Save in TextEdit".
  /// - Returns: "Clicked Save in TextEdit".
  static func leadingCapital(_ phrase: String) -> String {
    phrase.prefix(1)
      .uppercased() + phrase.dropFirst()
  }

  /// The phrase with its first letter in lower case, to follow a word.
  ///
  /// - Parameter phrase: The phrase, such as "A click may not have finished".
  /// - Returns: "a click may not have finished".
  static func leadingLower(_ phrase: String) -> String {
    phrase.prefix(1)
      .lowercased() + phrase.dropFirst()
  }
}
