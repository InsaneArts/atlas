import Foundation

@testable import AtlasService

/// One decoded packet: its header, metadata and whole payload.
struct DecodedFramePacket {
  /// The packet's header.
  var header: FramePipeHeader
  /// Its metadata.
  var metadata: Data
  /// Its whole payload.
  var payload = Data()
}
