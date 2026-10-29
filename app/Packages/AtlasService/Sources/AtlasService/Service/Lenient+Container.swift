import Foundation

/// Reads the service's fields leniently: a field that is missing or of the wrong type takes a safe default, so one odd
/// field never drops the object it is in.
extension KeyedDecodingContainer {
  /// A string, or nil when it is missing, of the wrong type or empty.
  ///
  /// - Parameter key: The field.
  /// - Returns: The string, or nil.
  func lenientText(_ key: Key) -> String? {
    (try? decodeIfPresent(String.self, forKey: key))
      .flatMap { $0.isEmpty ? nil : $0 }
  }

  /// Whether a field says true: false when it is missing or of the wrong type.
  ///
  /// - Parameter key: The field.
  /// - Returns: Whether it is true.
  func lenientFlag(_ key: Key) -> Bool {
    lenientBool(key) == true
  }

  /// A true or false, or nil when it is missing or of the wrong type, for a field whose absence says something.
  ///
  /// - Parameter key: The field.
  /// - Returns: The value, or nil.
  func lenientBool(_ key: Key) -> Bool? {
    try? decodeIfPresent(Bool.self, forKey: key)
  }

  /// A count: 0 when it is missing, of the wrong type or below 0.
  ///
  /// - Parameter key: The field.
  /// - Returns: The count.
  func lenientCount(_ key: Key) -> Int {
    max(0, (try? decodeIfPresent(Int.self, forKey: key)) ?? 0)
  }

  /// A moment written as milliseconds since 1970, or nil when it is missing, of the wrong type or not finite.
  ///
  /// - Parameter key: The field.
  /// - Returns: The moment, or nil.
  func lenientDate(milliseconds key: Key) -> Date? {
    (try? decodeIfPresent(Double.self, forKey: key))
      .flatMap { $0.isFinite ? Date(timeIntervalSince1970: $0 / 1000) : nil }
  }

  /// A list, each element decoded on its own, so one odd element is dropped and the rest are kept; none when the
  /// list is missing or of the wrong type.
  ///
  /// - Parameters:
  ///   - type: The elements' type.
  ///   - key: The field.
  /// - Returns: The elements that decoded, in order.
  func lenientList<Element: Decodable>(_ type: Element.Type, forKey key: Key) -> [Element] {
    (try? decodeIfPresent([Lenient<Element>].self, forKey: key))?
      .compactMap(\.value) ?? []
  }

  /// The names a list of apps gives, in order, each entry decoded on its own; an entry that names no app is left
  /// out.
  ///
  /// - Parameter key: The field.
  /// - Returns: The names.
  func lenientAppNames(_ key: Key) -> [String] {
    lenientList(LenientAppName.self, forKey: key)
      .compactMap(\.app)
      .filter { !$0.isEmpty }
  }
}
