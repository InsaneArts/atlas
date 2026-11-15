@testable import AtlasVMKit

extension ChangeID {
  /// The ID made of the two hexadecimal digits `pair`, 24 times over, as
  /// tests name their changes.
  static func repeating(_ pair: StaticString) -> ChangeID {
    // swift-format-ignore: NeverForceUnwrap
    // 24 pairs of lowercase hexadecimal digits are always a change's ID.
    ChangeID(String(repeating: "\(pair)", count: 24))!
  }
}
