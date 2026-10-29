import Foundation

/// The tile drawn for an app whose own icon cannot be read: the first letter of its name on a colour its name
/// chooses.
public struct AppMonogram: Equatable, Sendable {
  /// The letter, or digit, the tile shows.
  public var letter: String
  /// The tile's colour.
  public var tint: MonogramTint

  /// Creates a tile.
  ///
  /// - Parameters:
  ///   - letter: The letter it shows.
  ///   - tint: Its colour.
  public init(letter: String, tint: MonogramTint) {
    self.letter = letter
    self.tint = tint
  }

  /// The tile of an app's name: its first letter or digit, else its first character, in capitals as the person's
  /// language writes them, on the name's tint.
  ///
  /// - Parameters:
  ///   - name: The app's name.
  ///   - locale: The person's language, which says how a letter is capitalised.
  public init(name: String, locale: Locale = .current) {
    let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
    let first = trimmed.first { $0.isLetter || $0.isNumber } ?? trimmed.first
    let capital = first.map { letter in
      String(letter)
        .uppercased(with: locale)
    }
    self.init(letter: capital?.first.map(String.init) ?? "", tint: MonogramTint(name: trimmed))
  }
}
