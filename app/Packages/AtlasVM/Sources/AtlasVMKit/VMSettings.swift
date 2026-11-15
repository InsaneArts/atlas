/// The machine a VM is made with.
public struct VMSettings: Equatable, Sendable {
  /// Its processors.
  public var processors: Int
  /// Its memory, in bytes, taken whole when it starts.
  public var memoryBytes: UInt64
  /// Its disk's size, in bytes; the disk uses only what the guest fills.
  public var diskBytes: UInt64
  /// Its one display's width, in pixels.
  public var displayWidth: Int
  /// Its one display's height, in pixels.
  public var displayHeight: Int

  /// What every VM is made with: 4 processors, 8 GB of memory, a 128 GB
  /// disk and one 1920 by 1200 display; no sound.
  public static let standard = VMSettings(
    processors: 4,
    memoryBytes: 8 << 30,
    diskBytes: 128_000_000_000,
    displayWidth: 1920,
    displayHeight: 1200
  )
}
