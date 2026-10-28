/// One element of a list or a map the service writes, decoded on its own: an element that does not decode reads as
/// none, so one odd element never drops the rest.
struct Lenient<Wrapped: Decodable>: Decodable {
  /// The element, or nil when it did not decode.
  var value: Wrapped?

  /// Decodes the element, keeping nil when it is not one.
  ///
  /// - Parameter decoder: The decoder reading the element.
  init(from decoder: any Decoder) {
    value = try? Wrapped(from: decoder)
  }
}
