import Testing

@testable import AtlasVMKit

@Suite("The Mac's DHCP leases")
struct DHCPLeasesTests {
  @Test("a VM's address is its newest lease, its MAC address read without leading zeros")
  func address() {
    let leases = """
      {
      	name=AppleViMachine1
      	ip_address=192.168.64.3
      	hw_address=1,be:50:f7:c2:36:6a
      	lease=0x6af97000
      }
      {
      	name=AppleViMachine1
      	ip_address=192.168.64.4
      	hw_address=1,be:50:f7:c2:36:6a
      	lease=0x6af97418
      }
      {
      	name=Other
      	ip_address=192.168.64.9
      	hw_address=1,2:0:0:0:0:1
      	lease=0x6af97418
      }
      """
    #expect(DHCPLeases.address(of: "be:50:f7:c2:36:6a", in: leases) == "192.168.64.4")
    #expect(DHCPLeases.address(of: "02:00:00:00:00:01", in: leases) == "192.168.64.9")
    #expect(DHCPLeases.address(of: "be:50:f7:c2:36:6b", in: leases) == nil)
  }
}
