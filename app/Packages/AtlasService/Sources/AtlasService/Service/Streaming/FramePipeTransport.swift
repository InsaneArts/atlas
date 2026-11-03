import Foundation

/// One of the two connections to the frame service, as bytes: one read and one write at a time.
public protocol FramePipeTransport: Sendable {
  /// The next bytes, at most `limit`; empty once the lane ended.
  ///
  /// - Parameter limit: The most bytes to read.
  /// - Returns: The bytes read.
  /// - Throws: When the lane failed.
  func read(upTo limit: Int) async throws -> Data

  /// Writes all of `data`.
  ///
  /// - Parameter data: The bytes.
  /// - Throws: When the lane failed.
  func write(_ data: Data) async throws

  /// Ends the lane: a read under way returns at once, and nothing more is sent.
  func close()
}
