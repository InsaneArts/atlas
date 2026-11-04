import Foundation

/// What a lane's reader decoded.
enum FramePipeIncoming: Sendable {
  /// A packet's header and metadata, from the lane named.
  case packet(FramePipeLane, FramePipeHeader, Data)
  /// The next piece of the frame lane's current payload.
  case payload(Data)
}
