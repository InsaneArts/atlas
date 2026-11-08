import SwiftUI

/// The outline's quiet link, such as "120 pictures ›", "Show 112 more ›" or Show fewer: callout words in the secondary
/// grey in a 24-high capsule that fills under the pointer, its words turning primary.
struct OutlineLinkStyle: ButtonStyle {
  /// Whether a chevron follows the words.
  var chevron = true

  /// The link.
  ///
  /// - Parameter configuration: Its words and whether it is pressed.
  /// - Returns: The link.
  func makeBody(configuration: Configuration) -> some View {
    OutlineLinkBody(configuration: configuration, chevron: chevron)
  }
}
