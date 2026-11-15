/// The ID a station gives a change it asks for: 48 lowercase hexadecimal
/// digits, as Atlas's tokens are.
public struct ChangeID: Hashable, Codable, Sendable, CustomStringConvertible {
  /// Its digits.
  public let text: String

  /// `text` as a change's ID; none unless it is 48 lowercase hexadecimal
  /// digits.
  public init?(_ text: String) {
    let digits = text.utf8.allSatisfy { byte in
      (UInt8(ascii: "0")...UInt8(ascii: "9")).contains(byte)
        || (UInt8(ascii: "a")...UInt8(ascii: "f")).contains(byte)
    }
    guard text.utf8.count == 48, digits else { return nil }
    self.text = text
  }

  /// Read from its digits; anything else is refused.
  public init(from decoder: any Decoder) throws {
    let text = try decoder.singleValueContainer().decode(String.self)
    guard let id = ChangeID(text) else {
      throw DecodingError.dataCorrupted(
        .init(codingPath: decoder.codingPath, debugDescription: "not a change's ID")
      )
    }
    self = id
  }

  /// Written as its digits.
  public func encode(to encoder: any Encoder) throws {
    var container = encoder.singleValueContainer()
    try container.encode(text)
  }

  /// Its digits.
  public var description: String { text }
}
