/// The password a new VM's second administrator is made with: one the
/// person can type into the VM's window when macOS asks for an
/// administrator there. Twenty letters, each one of sixteen, so 80 bits at
/// random: lowercase only, none that reads like another (no i, l or o), and
/// only letters every common layout (QWERTY, QWERTZ and AZERTY) keeps on the
/// same key, so the key the person presses types it whatever their Mac's
/// layout. It is shown in four groups of five, and typed without spaces.
public enum AdminPassphrase {
  /// The letters it is made of.
  public static let letters = Array("bcdefghjknprstuv")
  /// How many groups it is shown in.
  public static let groups = 4
  /// How many letters each group holds.
  public static let groupLength = 5

  /// A new one, its letters drawn from `generator`.
  public static func make<Generator: RandomNumberGenerator>(using generator: inout Generator) -> String {
    let count = groups * groupLength
    return String(
      (0..<count).map { _ in
        letters[Int(generator.next(upperBound: UInt(letters.count)))]
      }
    )
  }

  /// `password` in the groups it is shown in; whole when it is not one of
  /// these, as an older VM's.
  public static func grouped(_ password: String) -> [String] {
    let letters = Array(password)
    guard letters.count == groups * groupLength else { return [password] }
    return stride(from: 0, to: letters.count, by: groupLength)
      .map { String(letters[$0..<$0 + groupLength]) }
  }
}
