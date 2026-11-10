extension ViewerClient {
  /// The secret a launch line gives: `secret ` and 64 lowercase hexadecimal digits, as the viewer service writes it
  /// to the app's standard input after the capability URL.
  ///
  /// - Parameter line: The line, surrounding whitespace allowed.
  /// - Returns: The secret, or nil when the line is not one.
  public static func appSecret(fromLine line: String) -> String? {
    let parts = line.trimmingCharacters(in: .whitespacesAndNewlines)
      .split(separator: " ", omittingEmptySubsequences: false)
    guard parts.count == 2, parts[0] == "secret" else { return nil }
    let secret = parts[1]
    guard secret.count == 64, secret.allSatisfy({ ("0"..."9").contains($0) || ("a"..."f").contains($0) }) else {
      return nil
    }
    return String(secret)
  }
}
