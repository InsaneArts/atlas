import Darwin
import Foundation
import Testing

@testable import AtlasVMKit

@Suite("The splice")
struct SpliceTests {
  @Test("bytes go both ways, and each way's end reaches the other end")
  func bothWays() throws {
    let (station, joinedStation) = Sockets.pair()
    let (guest, joinedGuest) = Sockets.pair()
    Splice.join(joinedStation, joinedGuest)

    Sockets.say("from the station", to: station)
    shutdown(station, SHUT_WR)
    #expect(Sockets.heard(from: guest) == "from the station")

    Sockets.say("from the guest", to: guest)
    shutdown(guest, SHUT_WR)
    #expect(Sockets.heard(from: station) == "from the guest")
    close(station)
    close(guest)
  }
}
