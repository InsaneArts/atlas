import Foundation

/// What the packet decoder has read.
public enum FramePacketItem: Sendable, Equatable {
  /// A packet's header and its whole metadata; its payload, if any, follows in pieces.
  case packet(FramePipeHeader, Data)
  /// The next piece of the current packet's payload, at most ``FramePipeLimits/chunk`` bytes.
  case payload(Data)
}
