/// The Mac's DHCP leases, as `/var/db/dhcpd_leases` keeps them for its VMs:
/// where a VM of a given MAC address answers.
public enum DHCPLeases {
  /// The address leased last to `mac` in `text`; none when there is none.
  public static func address(of mac: String, in text: String) -> String? {
    let wanted = normalized(mac)
    var best: (expiry: UInt64, address: String)?
    for block in text.components(separatedBy: "}") {
      var fields: [String: String] = [:]
      for line in block.split(separator: "\n") {
        let parts = line.trimmingCharacters(in: .whitespaces)
          .split(separator: "=", maxSplits: 1)
        if parts.count == 2 { fields[String(parts[0])] = String(parts[1]) }
      }
      guard let hardware = fields["hw_address"], let address = fields["ip_address"] else { continue }
      // Written as its type, then the address: `1,be:50:f7:c2:36:6a`.
      let named =
        hardware.split(separator: ",", maxSplits: 1).last
        .map(String.init) ?? hardware
      guard normalized(named) == wanted else { continue }
      let expiry = fields["lease"].flatMap { UInt64($0.dropFirst(2), radix: 16) } ?? 0
      if best.map({ expiry >= $0.expiry }) ?? true {
        best = (expiry, address)
      }
    }
    return best?.address
  }

  /// `mac` as the leases write it: lowercase, each octet without leading zeros.
  private static func normalized(_ mac: String) -> String {
    mac
      .lowercased()
      .split(separator: ":")
      .map { octet in
        let trimmed = octet.drop { $0 == "0" }
        return trimmed.isEmpty ? "0" : String(trimmed)
      }
      .joined(separator: ":")
  }
}
