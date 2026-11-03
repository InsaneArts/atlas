/// Which connection a packet may travel on.
public enum FramePipeLane: Sendable, Hashable {
  /// Requests, acknowledgements and control between a client and the broker.
  case control
  /// Pictures, after the pair's attach handshake.
  case frame
  /// App hosts and cleanup of old observers; never pictures or input.
  case management
}
