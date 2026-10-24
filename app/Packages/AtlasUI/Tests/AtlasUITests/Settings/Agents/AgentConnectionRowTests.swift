import Testing

@testable import AtlasUI

@Suite("AgentConnectionRow")
struct AgentConnectionRowTests {
  @Test("Connect and Connecting… are one button that keeps its place, working and waiting while the connection runs")
  func connectAndConnectingAreOneButton() {
    #expect(AgentConnectionRow.trailing(.connect(isEnabled: true)) == .button(isWorking: false, isEnabled: true))
    #expect(AgentConnectionRow.trailing(.connect(isEnabled: false)) == .button(isWorking: false, isEnabled: false))
    #expect(AgentConnectionRow.trailing(.connecting) == .button(isWorking: true, isEnabled: false))
    #expect(AgentConnectionRow.trailing(.connected) == .connected)
    #expect(AgentConnectionRow.trailing(.note("Not installed")) == .note("Not installed"))
  }
}
