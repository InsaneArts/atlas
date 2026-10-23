import Foundation

/// What a new helper account's short name must be: the rule `atlas onboarding create` applies
/// (`valid_account_name` in `src/lib/init.rs`), checked here as the person types so the form never sends a name
/// the command line refuses. Each problem says what is wrong in plain words and suggests a name that works.
package enum AccountNameRule {
  /// The longest short name macOS takes: 31 characters.
  static let maxLength = 31

  /// What is wrong with the short name, if anything; nothing while it is empty, which the form's button waits on.
  ///
  /// - Parameter name: The short name typed.
  /// - Returns: One line saying what to change, with a name to try, or `nil` when there is nothing to say yet.
  package static func problem(_ name: String) -> String? {
    if name.isEmpty { return nil }
    let reason: String
    if name.contains(where: \.isWhitespace) {
      reason = "Account names can't have spaces."
    } else if !name.utf8.allSatisfy(allowed) {
      reason = "Account names can only use letters, numbers, dots, dashes and underscores."
    } else if name.hasPrefix(".") || name.hasPrefix("-") {
      reason = "Account names start with a letter or a number."
    } else if name.utf8.count > maxLength {
      reason = "Account names can be at most 31 characters."
    } else if name.utf8.allSatisfy(isDigit) {
      reason = "Account names need at least one letter."
    } else {
      return nil
    }
    return "\(reason) Try “\(suggestion(for: name))”."
  }

  /// A name that works, made from what was typed: lowercase, accents dropped, spaces and other marks made dashes,
  /// no dash or dot to start, at most 31 characters, "atlas" before digits alone; "atlas" when nothing of it is left.
  ///
  /// - Parameter name: The short name typed.
  /// - Returns: A short name the rule accepts.
  package static func suggestion(for name: String) -> String {
    let plain = name.folding(options: [.diacriticInsensitive, .caseInsensitive], locale: Locale(identifier: "en"))
    var made = ""
    for byte in plain.utf8 {
      if allowed(byte) {
        made.append(Character(UnicodeScalar(byte)))
      } else if !made.hasSuffix("-") {
        made.append("-")
      }
    }
    let trimmed = String(
      made.drop { $0 == "-" || $0 == "." }
        .prefix(maxLength)
    )
    let tidy = String(
      trimmed.reversed()
        .drop { $0 == "-" || $0 == "." }
        .reversed()
    )
    if tidy.isEmpty { return "atlas" }
    // macOS reads a name of only digits as a user ID.
    return tidy.utf8.allSatisfy(isDigit) ? String("atlas\(tidy)".prefix(maxLength)) : tidy
  }

  /// Whether a byte is an ASCII digit.
  ///
  /// - Parameter byte: One byte of the name's UTF-8.
  /// - Returns: Whether it is 0 to 9.
  private static func isDigit(_ byte: UInt8) -> Bool {
    (UInt8(ascii: "0")...UInt8(ascii: "9")).contains(byte)
  }

  /// Whether a byte may be in a short name: an ASCII letter or digit, '.', '_' or '-'.
  ///
  /// - Parameter byte: One byte of the name's UTF-8.
  /// - Returns: Whether it is allowed.
  private static func allowed(_ byte: UInt8) -> Bool {
    (byte >= UInt8(ascii: "0") && byte <= UInt8(ascii: "9")) || (byte >= UInt8(ascii: "A") && byte <= UInt8(ascii: "Z"))
      || (byte >= UInt8(ascii: "a") && byte <= UInt8(ascii: "z")) || byte == UInt8(ascii: ".")
      || byte == UInt8(ascii: "_") || byte == UInt8(ascii: "-")
  }
}
