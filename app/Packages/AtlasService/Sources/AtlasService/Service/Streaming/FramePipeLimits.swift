/// The frame pipe's fixed bounds, the same on both sides of the wire.
public enum FramePipeLimits {
  /// The fixed prefix's length, in bytes.
  public static let prefix = 32
  /// The protocol major version this side speaks.
  public static let major: UInt16 = 1
  /// The largest metadata object, in bytes.
  public static let metadata = 8_192
  /// The largest packet without a payload, prefix included, in bytes.
  public static let control = 16_384
  /// The largest payload: one 4,096 × 4,096 RGB picture.
  public static let payload: UInt64 = 4_096 * 4_096 * 3
  /// The largest payload piece the decoder hands out at once, in bytes.
  public static let chunk = 65_536
  /// The longest side of a picture, in pixels.
  public static let side: UInt32 = 4_096
  /// The most rectangles one delta carries.
  public static let rectangles: UInt32 = 256
  /// The bytes of one rectangle's record: x, y, width and height.
  public static let rectangleRecord = 16
  /// The deepest nesting of metadata objects and arrays.
  public static let depth = 8
  /// The most keys in one metadata object, nested ones included.
  public static let keys = 512
  /// The longest key, in UTF-8 bytes once decoded.
  public static let key = 128
  /// The longest string value, in UTF-8 bytes once decoded.
  public static let string = 1_024
}
