/// A phrase's first letter, raised to lead a line, as the service's words and the app's own are joined.
extension Words {
  /// The phrase with its first letter in capitals, to lead a line.
  ///
  /// - Parameter phrase: The phrase, such as "clicked Save in TextEdit".
  /// - Returns: "Clicked Save in TextEdit".
  static func leadingCapital(_ phrase: String) -> String {
    phrase.prefix(1)
      .uppercased() + phrase.dropFirst()
  }
}
