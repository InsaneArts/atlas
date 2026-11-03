/// Every packet kind, by its code on the wire.
public enum FramePacketKind: UInt16, Sendable, CaseIterable {
  /// Code 1.
  case hello = 1
  /// Code 2.
  case welcome = 2
  /// Code 3.
  case attach = 3
  /// Code 4.
  case attached = 4
  /// Code 5.
  case pairConfirm = 5
  /// Code 6.
  case pairReady = 6
  /// Code 7.
  case subscribe = 7
  /// Code 8.
  case subscribed = 8
  /// Code 9.
  case credit = 9
  /// Code 10.
  case creditAccepted = 10
  /// Code 11.
  case fullFrame = 11
  /// Code 12.
  case deltaFrame = 12
  /// Code 13.
  case unchangedFrame = 13
  /// Code 14.
  case decoded = 14
  /// Code 15.
  case decodedAck = 15
  /// Code 16.
  case commit = 16
  /// Code 17.
  case commitAck = 17
  /// Code 18.
  case drop = 18
  /// Code 19.
  case dropAck = 19
  /// Code 20.
  case resync = 20
  /// Code 21.
  case resyncAck = 21
  /// Code 22.
  case acquire = 22
  /// Code 23.
  case acquired = 23
  /// Code 24.
  case renew = 24
  /// Code 25.
  case renewed = 25
  /// Code 26.
  case input = 26
  /// Code 27.
  case inputAck = 27
  /// Code 28.
  case release = 28
  /// Code 29.
  case released = 29
  /// Code 30.
  case stop = 30
  /// Code 31.
  case stopResult = 31
  /// Code 32.
  case suspend = 32
  /// Code 33.
  case suspended = 33
  /// Code 34.
  case close = 34
  /// Code 35.
  case closed = 35
  /// Code 36.
  case creditExpired = 36
  /// Code 37.
  case state = 37
  /// Code 38.
  case error = 38
  /// Code 39.
  case requestDiagnostics = 39
  /// Code 40.
  case diagnosticsReply = 40
  /// Code 41.
  case recoveryHello = 41
  /// Code 42.
  case recoveryReady = 42
  /// Code 43.
  case cleanup = 43
  /// Code 44.
  case cleanupResult = 44
  /// Code 45.
  case registerAppHost = 45
  /// Code 46.
  case appHostReady = 46
  /// Code 47.
  case appCommand = 47
  /// Code 48.
  case appCommandAck = 48
  /// Code 49.
  case appHostClosing = 49
  /// Code 50.
  case appHostClosed = 50

  /// The lanes the kind may travel on.
  public var lanes: Set<FramePipeLane> {
    switch self {
    case .error: [.control, .frame, .management]
    case .attach, .attached, .fullFrame, .deltaFrame, .unchangedFrame: [.frame]
    case .recoveryHello, .recoveryReady, .cleanup, .cleanupResult, .registerAppHost, .appHostReady, .appCommand,
      .appCommandAck, .appHostClosing, .appHostClosed:
      [.management]
    default: [.control]
    }
  }

  /// Whether the kind carries a binary payload: only changed pictures do.
  public var hasPayload: Bool {
    self == .fullFrame || self == .deltaFrame
  }

  /// Whether the kind may have request ID zero: an unsolicited state change, or an error that answers no request.
  var isUnsolicited: Bool {
    self == .state || self == .error
  }
}
