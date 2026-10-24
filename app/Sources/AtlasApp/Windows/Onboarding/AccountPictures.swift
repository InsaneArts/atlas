import AppKit
import AtlasUI
import Collaboration

/// An account's picture as macOS shows it at login, read from the Mac's own accounts by its short name. Only read:
/// nothing about the account changes.
enum AccountPictures {
  /// The account's picture.
  ///
  /// - Parameter name: The account's short name.
  /// - Returns: Its picture, or nil where the account or its picture is not found.
  @MainActor static func picture(of name: String) -> IconImage? {
    guard !name.isEmpty,
      let identity = CBIdentity(name: name, authority: .default()) as? CBUserIdentity,
      let image = identity.image?.cgImage(forProposedRect: nil, context: nil, hints: nil)
    else { return nil }
    return IconImage(id: name, image: image)
  }
}
