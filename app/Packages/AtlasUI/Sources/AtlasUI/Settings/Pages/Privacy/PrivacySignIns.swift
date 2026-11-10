/// The Privacy page's "Sign-Ins" row: whether agents may copy sign-ins into the helper.
struct PrivacySignIns: Identifiable {
  /// Whether they may.
  let isOn: Bool

  /// The section holds one row.
  var id: Int { 0 }
}
