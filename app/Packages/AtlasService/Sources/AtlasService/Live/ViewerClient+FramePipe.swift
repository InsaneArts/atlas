import Foundation

extension ViewerClient {
  /// Reads `pipe.json`: how to pair with the frame service this viewer runs.
  ///
  /// - Returns: The descriptor, or nil when the viewer runs none.
  /// - Throws: A transport error, or ``ServiceError`` for an unreadable answer.
  public func framePipe() async throws -> FramePipeDescriptor? {
    let (data, response) = try await session.data(for: request("pipe.json"))
    switch (response as? HTTPURLResponse)?.statusCode {
    case 200:
      do {
        return try JSONDecoder()
          .decode(FramePipeDescriptor.self, from: data)
      } catch {
        throw ServiceError("The frame service's description is unreadable")
      }
    case 404: return nil
    default: throw Self.failure(data, fallback: "No frame service")
    }
  }
}
