/// One element of a list this app only counts: any value decodes, so every element the service lists counts.
struct LenientEntry: Decodable {
  /// Decodes an element, reading nothing of it.
  ///
  /// - Parameter decoder: The decoder reading the element.
  init(from decoder: any Decoder) {}
}
