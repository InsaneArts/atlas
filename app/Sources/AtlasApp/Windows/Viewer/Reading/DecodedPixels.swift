import AtlasUI
import Foundation

/// A picture's packed RGB pixels and what they decoded to, kept so the very same pixels again reuse the image.
struct DecodedPixels: Sendable {
  /// The pixels, `width * 3` bytes a row.
  let rgb: Data
  /// The picture they made.
  let decoded: DesktopPicture.Decoded
}
