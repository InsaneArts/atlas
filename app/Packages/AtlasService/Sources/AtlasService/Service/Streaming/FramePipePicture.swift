import Foundation

/// One verified picture of the helper from the frame service: packed RGB, three bytes a pixel, with what it shows.
public struct FramePipePicture: Sendable {
  /// The pixels, top to bottom and left to right, `width * 3` bytes a row.
  public let rgb: Data
  /// The width, in pixels.
  public let width: Int
  /// The height, in pixels.
  public let height: Int
  /// The helper's display size, in points.
  public let display: ViewerStatus.Size
  /// The key input names this picture by once it is committed.
  public let frameKey: String
  /// Changes whenever the helper's session or the stream's geometry changes.
  public let revision: String
  /// How old it was when the service sent it, in seconds.
  public let ageAtHeader: Double
  /// The credit it answered.
  let credit: UInt64
  /// The exact reference it is settled by, as its JSON object.
  let reference: Data
}
