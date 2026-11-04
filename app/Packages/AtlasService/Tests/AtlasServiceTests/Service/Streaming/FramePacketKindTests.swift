import Testing

@testable import AtlasService

@Suite("Frame packet kinds")
struct FramePacketKindTests {
  @Test("every kind has its planned code and lanes")
  func codesAndLanes() {
    let control: Set<FramePipeLane> = [.control]
    let frame: Set<FramePipeLane> = [.frame]
    let management: Set<FramePipeLane> = [.management]
    let planned: [(FramePacketKind, UInt16, Set<FramePipeLane>)] = [
      (.hello, 1, control), (.welcome, 2, control), (.attach, 3, frame), (.attached, 4, frame),
      (.pairConfirm, 5, control), (.pairReady, 6, control), (.subscribe, 7, control), (.subscribed, 8, control),
      (.credit, 9, control), (.creditAccepted, 10, control), (.fullFrame, 11, frame), (.deltaFrame, 12, frame),
      (.unchangedFrame, 13, frame), (.decoded, 14, control), (.decodedAck, 15, control), (.commit, 16, control),
      (.commitAck, 17, control), (.drop, 18, control), (.dropAck, 19, control), (.resync, 20, control),
      (.resyncAck, 21, control), (.acquire, 22, control), (.acquired, 23, control), (.renew, 24, control),
      (.renewed, 25, control), (.input, 26, control), (.inputAck, 27, control), (.release, 28, control),
      (.released, 29, control), (.stop, 30, control), (.stopResult, 31, control), (.suspend, 32, control),
      (.suspended, 33, control), (.close, 34, control), (.closed, 35, control), (.creditExpired, 36, control),
      (.state, 37, control), (.error, 38, [.control, .frame, .management]), (.requestDiagnostics, 39, control),
      (.diagnosticsReply, 40, control), (.recoveryHello, 41, management), (.recoveryReady, 42, management),
      (.cleanup, 43, management), (.cleanupResult, 44, management), (.registerAppHost, 45, management),
      (.appHostReady, 46, management), (.appCommand, 47, management), (.appCommandAck, 48, management),
      (.appHostClosing, 49, management), (.appHostClosed, 50, management),
    ]
    #expect(planned.count == 50)
    #expect(FramePacketKind.allCases.count == 50)
    for (kind, code, lanes) in planned {
      #expect(kind.rawValue == code, "\(kind)")
      #expect(FramePacketKind(rawValue: code) == kind)
      #expect(kind.lanes == lanes, "\(kind)")
      #expect(kind.hasPayload == (kind == .fullFrame || kind == .deltaFrame))
    }
    #expect(FramePacketKind(rawValue: 0) == nil)
    #expect(FramePacketKind(rawValue: 51) == nil)
  }
}
