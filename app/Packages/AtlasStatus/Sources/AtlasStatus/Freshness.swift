/// How old a picture may be, in seconds: the header and control agree on it.
public enum Freshness {
  /// Younger than this reads as live.
  public static let live: Double = 3
  /// This old, the picture is stale: the header says so and control ends.
  public static let stale: Double = 6.5
}
