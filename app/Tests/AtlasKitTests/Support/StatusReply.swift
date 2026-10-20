import Foundation

@testable import AtlasKit

/// A status reply as the viewer service writes it.
///
/// - Parameter reply: The status's JSON fields.
/// - Returns: The decoded status.
/// - Throws: A decoding error when the fields are not a status.
func status(_ reply: [String: Any]) throws -> ViewerStatus {
  try JSONDecoder().decode(ViewerStatus.self, from: JSONSerialization.data(withJSONObject: reply))
}
